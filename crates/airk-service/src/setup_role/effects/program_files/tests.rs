// SPDX-License-Identifier: Apache-2.0
use super::*;
use sha2::{Digest, Sha256};

fn hash(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
fn write(folder: &std::path::Path, name: &str, bytes: &[u8]) {
    airk_ipc::private_record::write_private(folder, name, bytes, 4096, true).unwrap();
}

#[test]
fn owned_payload_removal_preserves_changed_files_and_is_idempotent() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let path = owner.path().canonicalize()?.join("program");
    airk_ipc::endpoint::prepare_root(&path)?;
    let root = OpenedDirectory::open(&path)?;
    write(&path, "payload", b"modified");
    assert_eq!(
        remove(&root, "payload", hash(b"original"), Some(8))?,
        Removal::Changed
    );
    assert_eq!(std::fs::read(path.join("payload"))?, b"modified");
    write(&path, "payload", b"original");
    assert_eq!(
        remove(&root, "payload", hash(b"original"), Some(8))?,
        Removal::Removed
    );
    assert!(!path.join("payload").exists());
    assert_eq!(
        remove(&root, "payload", hash(b"original"), Some(8))?,
        Removal::Absent
    );
    Ok(())
}

#[test]
fn owned_payload_removal_refuses_hardlink_aliases_and_traversal() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let path = owner.path().canonicalize()?.join("program");
    airk_ipc::endpoint::prepare_root(&path)?;
    write(&path, "payload", b"original");
    std::fs::hard_link(path.join("payload"), owner.path().join("source-alias"))?;
    let root = OpenedDirectory::open(&path)?;
    let outcome = remove(&root, "payload", hash(b"original"), Some(8));
    assert!(outcome.is_err() || outcome? == Removal::Changed);
    assert_eq!(
        std::fs::read(owner.path().join("source-alias"))?,
        b"original"
    );
    assert_eq!(std::fs::read(path.join("payload"))?, b"original");
    assert!(remove(&root, "../source-alias", hash(b"original"), Some(8)).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_payload_removal_stays_in_original_root_and_refuses_symlinked_ancestors() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let path = owner.path().canonicalize()?;
    let program = path.join("program");
    airk_ipc::endpoint::prepare_root(&program)?;
    write(&program, "payload", b"original");
    let root = OpenedDirectory::open(&program)?;
    std::fs::rename(&program, path.join("original-program"))?;
    airk_ipc::endpoint::prepare_root(&program)?;
    write(&program, "payload", b"replacement source");
    assert_eq!(
        remove(&root, "payload", hash(b"original"), Some(8))?,
        Removal::Removed
    );
    assert_eq!(
        std::fs::read(program.join("payload"))?,
        b"replacement source"
    );
    std::os::unix::fs::symlink(&program, path.join("original-program/lib"))?;
    assert!(remove(&root, "lib/payload", hash(b"replacement source"), Some(18)).is_err());
    assert_eq!(
        std::fs::read(program.join("payload"))?,
        b"replacement source"
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn owned_payload_with_foreign_alternate_stream_is_preserved() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("program");
    airk_ipc::endpoint::prepare_root(&path)?;
    write(&path, "payload", b"original");
    std::fs::write(path.join("payload:foreign"), b"preserve foreign stream")?;
    let root = OpenedDirectory::open(&path)?;
    assert!(remove(&root, "payload", hash(b"original"), Some(8)).is_err());
    assert_eq!(
        std::fs::read(path.join("payload:foreign"))?,
        b"preserve foreign stream"
    );
    assert_eq!(std::fs::read(path.join("payload"))?, b"original");
    Ok(())
}
