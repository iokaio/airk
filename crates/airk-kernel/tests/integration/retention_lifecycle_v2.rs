// SPDX-License-Identifier: Apache-2.0
//! Current object ownership replaces generation-retention policy and idle leases.
use super::current_inspection::{InspectCurrent, read_current};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::Result;
use std::fs;

#[test]
fn two_hundred_current_edits_reach_object_and_database_plateau() -> Result<()> {
    let root = tempfile::tempdir()?;
    let other_root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    let other = Kernel::open_collection(other_root.path(), Some(data.path().into()), "other")?;
    fs::write(
        other_root.path().join("other.txt"),
        "independent immutable source",
    )?;
    other.index()?;
    let other_token = other.status()?.publication.unwrap().token;
    let directory = data.path().join("collections-1.2/default");
    let mut steady_bytes = 0;
    for index in 0..200 {
        let text = format!("original evidence edit {index:03} amber cedar forest river\n");
        fs::write(root.path().join("note.txt"), &text)?;
        kernel.index()?;
        assert_eq!(kernel.read_file("note.txt", 1, 1, 8192)?.hits[0].text, text);
        for objects in ["objects/sources", "objects/artifacts", "current-metadata"] {
            assert_eq!(
                fs::read_dir(directory.join(objects))?.count(),
                1,
                "{objects}, edit {index}"
            );
        }
        assert_eq!(
            fs::read_dir(&directory)?
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().starts_with("lexical-"))
                .count(),
            0
        );
        let bytes = fs::metadata(directory.join("current.sqlite3"))?.len();
        if index == 100 {
            steady_bytes = bytes;
        }
        if index > 100 {
            assert!(
                bytes <= steady_bytes + (1 << 20),
                "database failed to reach a bounded plateau"
            );
        }
    }
    let token = kernel.status()?.publication.unwrap().token;
    assert_eq!(kernel.maintain_current()?.removed_files, 0);
    assert_eq!(kernel.status()?.publication.unwrap().token, token);
    assert_eq!(other.status()?.publication.unwrap().token, other_token);
    assert_eq!(
        fs::read_to_string(other_root.path().join("other.txt"))?,
        "independent immutable source"
    );
    Ok(())
}

#[test]
fn corrupt_live_ownership_refuses_cleanup_without_deleting_unowned_objects() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("note.txt"),
        "authenticated current original source",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let observation = store.current_test_catalog()?.unwrap();
    let path = store.directory().join("current-metadata");
    let live = fs::read_dir(&path)?.next().unwrap()?.path();
    let saved = fs::read(&live)?;
    let orphan = path.join(format!("{}.json", "b".repeat(64)));
    fs::write(&orphan, b"orphan fixture")?;
    fs::write(&live, vec![b'x'; saved.len()])?;
    assert!(kernel.maintain_current().is_err());
    assert!(orphan.exists());
    fs::write(live, saved)?;
    assert_eq!(
        read_current(&store)?.head().sequence,
        observation.token.generation
    );
    let read = read_current(&store)?;
    assert!(kernel.maintain_current().is_err());
    assert!(orphan.exists());
    drop(read);
    assert_eq!(kernel.maintain_current()?.removed_files, 1);
    assert!(!orphan.exists());
    assert_eq!(
        kernel.status()?.publication.unwrap().token,
        observation.token
    );
    Ok(())
}
