// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::json;
use std::path::PathBuf;

fn id(byte: u8) -> Incarnation {
    Incarnation::from_bytes([byte; 16])
}

fn names(root: &Path) -> Result<Vec<String>> {
    let mut names = std::fs::read_dir(root)?
        .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
        .collect::<Result<Vec<_>>>()?;
    names.sort();
    Ok(names)
}

/// A pre-B `http-1` folder written by hand: the ledger, two 103-byte tokens,
/// a retirement marker, and the TLS and endpoint records that stay.
fn old_install(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root)?;
    let ledger = json!({"schema":1,"binding":{"installation":"01".repeat(32),"user":"u",
        "namespace":id(2)},"records":[{"id":id(10),"revoked":false,"expires_unix_ms":1,
        "verifier":"0a".repeat(32)}]});
    std::fs::write(root.join("credentials.json"), serde_json::to_vec(&ledger)?)?;
    for seed in [10, 11] {
        std::fs::write(
            root.join(format!("credential-{}.token", id(seed))),
            format!("airk1.{}.{}", id(seed), "f".repeat(64)),
        )?;
    }
    std::fs::write(
        root.join(format!("credential-{}.token", id(12))),
        serde_json::to_vec(&json!({"schema":1,"id":id(12),"revoked":true}))?,
    )?;
    Ok(())
}

#[test]
fn an_old_ledger_and_its_token_files_are_removed_and_other_records_stay() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().join("http-1");
    old_install(&root)?;
    std::fs::write(root.join("ca.pem"), b"public ca")?;
    std::fs::write(root.join("endpoint.json"), b"{}")?;
    // Look-alike names that are not exact credential names stay.
    std::fs::write(root.join("credential-not-an-id.token"), b"x")?;
    std::fs::write(root.join(format!("credential-{}.token.part", id(13))), b"x")?;
    std::fs::write(root.join("credentials.json.bak"), b"x")?;
    // A directory under a token name is not a file: it stays.
    std::fs::create_dir(root.join(format!("credential-{}.token", id(14))))?;
    let removed = remove(&root)?;
    assert_eq!(
        removed,
        RetiredCredentials {
            ledger: true,
            tokens: 3,
            folder: false
        }
    );
    assert_eq!(
        names(&root)?,
        vec![
            "ca.pem".to_owned(),
            format!("credential-{}.token.part", id(13)),
            format!("credential-{}.token", id(14)),
            "credential-not-an-id.token".to_owned(),
            "credentials.json.bak".to_owned(),
            "endpoint.json".to_owned(),
        ]
    );
    // A second pass finds nothing more.
    assert_eq!(remove(&root)?, RetiredCredentials::default());
    Ok(())
}

#[test]
fn a_folder_holding_only_the_ledger_and_tokens_is_removed() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().join("http-1");
    old_install(&root)?;
    let removed = remove(&root)?;
    assert_eq!(
        removed,
        RetiredCredentials {
            ledger: true,
            tokens: 3,
            folder: true
        }
    );
    assert!(!root.exists());
    // A missing folder is nothing to do, and a file in its place refuses.
    assert_eq!(remove(&root)?, RetiredCredentials::default());
    std::fs::write(&root, b"not a folder")?;
    assert!(remove(&root).is_err());
    assert_eq!(std::fs::read(&root)?, b"not a folder");
    Ok(())
}

#[test]
fn nothing_outside_the_folder_is_touched_and_links_are_not_followed() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let control = temporary.path().join("control");
    let root = control.join("http-1");
    old_install(&root)?;
    // The same names one level up and in a sibling folder are not this
    // folder's: they stay byte-identical.
    let outside: Vec<PathBuf> = vec![
        control.join("credentials.json"),
        control.join(format!("credential-{}.token", id(10))),
        temporary.path().join("other").join("credentials.json"),
    ];
    for path in &outside {
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, b"outside")?;
    }
    // A link under an exact credential name, pointing outside, is not a
    // regular file: neither it nor its target is removed. Creating a link
    // needs a privilege on Windows; without it this part is not exercised.
    let target = temporary.path().join("other").join("target.token");
    std::fs::write(&target, b"target")?;
    let link = root.join(format!("credential-{}.token", id(15)));
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(&target, &link).is_ok();
    #[cfg(windows)]
    let linked = std::os::windows::fs::symlink_file(&target, &link).is_ok();
    let removed = remove(&root)?;
    assert!(removed.ledger);
    assert_eq!(removed.tokens, 3);
    assert_eq!(removed.folder, !linked);
    for path in &outside {
        assert_eq!(std::fs::read(path)?, b"outside");
    }
    assert_eq!(std::fs::read(&target)?, b"target");
    if linked {
        assert!(std::fs::symlink_metadata(&link)?.file_type().is_symlink());
    }
    Ok(())
}
