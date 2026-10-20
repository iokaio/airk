// SPDX-License-Identifier: Apache-2.0
//! Actual runtime publication, request lifetimes and immutable evidence checks.
use super::current_inspection::{InspectCurrent, read_current};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::Result;
use std::fs;

#[test]
fn initialization_noop_edit_full_rebuild_and_idle_readers_use_one_current_head() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("note.txt"), "original indexed evidence\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.initialize()?;
    assert_eq!(
        kernel.status()?.files,
        0,
        "initialize does not scan sources"
    );
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    kernel.index()?;
    let before = store.current_test_catalog()?.unwrap();
    assert_eq!(kernel.index()?.generation, before.token.generation);
    let idle = Kernel::open(root.path(), Some(data.path().into()))?;
    idle.read_file("note.txt", 1, 1, 8192)?;
    fs::write(root.path().join("note.txt"), "updated indexed evidence\n")?;
    kernel.index()?;
    let updated = store.current_test_catalog()?.unwrap();
    let old = &before.inventory.as_ref().unwrap().files["note.txt"].state;
    let new = &updated.inventory.as_ref().unwrap().files["note.txt"].state;
    assert_eq!(new.artifact_id, old.artifact_id);
    assert!(new.stamps.content_generation > old.stamps.content_generation);
    assert_eq!(new.stamps.first_generation, old.stamps.first_generation);
    kernel.reindex()?;
    let rebuilt = store.current_test_catalog()?.unwrap();
    assert_eq!(&rebuilt.inventory.unwrap().files["note.txt"].state, new);
    assert_eq!(
        idle.read_file("note.txt", 1, 1, 8192)?.hits[0].text,
        "updated indexed evidence\n"
    );
    assert!(!store.directory().join("generations").exists());
    assert_eq!(
        fs::read_dir(store.directory().join("objects/sources"))?.count(),
        1
    );
    Ok(())
}

#[test]
fn metadata_reads_do_not_load_source_objects_and_detect_corrupt_metadata_on_every_request()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("note.txt"), "original indexed evidence\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let read = read_current(&store)?;
    let artifact = read.artifact("note.txt")?.unwrap();
    drop(read);
    let before = kernel.file_info("note.txt")?;
    fs::write(
        store
            .directory()
            .join("objects/artifacts")
            .join(&artifact.retained.unwrap().object),
        b"invalid derived fixture",
    )?;
    assert_eq!(
        kernel.file_info("note.txt")?.content_hash,
        before.content_hash
    );
    assert!(kernel.read_file("note.txt", 1, 1, 8192).is_err());
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    assert_eq!(reopened.file_info("note.txt")?.indexed_lines, 1);
    let metadata = fs::read_dir(store.directory().join("current-metadata"))?
        .next()
        .unwrap()?
        .path();
    let bytes = fs::read(&metadata)?;
    fs::write(metadata, vec![b'x'; bytes.len()])?;
    assert!(kernel.file_info("note.txt").is_err());
    assert!(reopened.file_info("note.txt").is_err());
    Ok(())
}

#[test]
fn concurrent_requests_return_coherent_current_metadata_or_explicit_busy() -> Result<()> {
    use sha2::{Digest, Sha256};
    use std::sync::{Arc, Barrier};
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let texts: Vec<_> = (0..5)
        .map(|n| format!("Observed revision {n}.\n{}", "Additional line.\n".repeat(n)))
        .collect();
    let expected: Vec<_> = texts
        .iter()
        .map(|text| {
            (
                format!("{:x}", Sha256::digest(text.as_bytes())),
                text.len(),
                text.lines().count(),
            )
        })
        .collect();
    fs::write(root.path().join("note.txt"), &texts[0])?;
    let writer = Kernel::open(root.path(), Some(data.path().into()))?;
    writer.index()?;
    let reader = Kernel::open(root.path(), Some(data.path().into()))?;
    let barrier = Arc::new(Barrier::new(2));
    let start = barrier.clone();
    let thread = std::thread::spawn(move || -> Result<usize> {
        start.wait();
        let mut successful = 0;
        for _ in 0..100 {
            match reader.file_info("note.txt") {
                Ok(info) => {
                    assert!(
                        expected
                            .iter()
                            .any(|(hash, bytes, lines)| hash == &info.content_hash
                                && *bytes == info.source_bytes
                                && *lines == info.indexed_lines)
                    );
                    successful += 1;
                }
                Err(error) => assert!(
                    format!("{error:#}").contains("busy")
                        || format!("{error:#}").contains("applying"),
                    "{error:#}"
                ),
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        Ok(successful)
    });
    barrier.wait();
    for text in &texts[1..] {
        fs::write(root.path().join("note.txt"), text)?;
        let started = std::time::Instant::now();
        loop {
            match writer.index() {
                Ok(_) => break,
                Err(error) => {
                    assert!(format!("{error:#}").contains("active readers"), "{error:#}");
                    assert!(started.elapsed().as_secs() < 30);
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
            }
        }
    }
    assert!(thread.join().unwrap()? > 0);
    assert_eq!(
        writer
            .read_file("note.txt", 1, 5, 8192)?
            .hits
            .iter()
            .map(|h| h.text.as_str())
            .collect::<String>(),
        texts[4]
    );
    Ok(())
}
