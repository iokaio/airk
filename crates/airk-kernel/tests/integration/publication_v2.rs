// SPDX-License-Identifier: Apache-2.0
use airk_core::{ArtifactSource, changes::PublicationReason};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::{Context, Result};
use std::fs;

#[test]
fn initialization_noop_edits_and_full_reindex_share_one_publication_chain() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.initialize()?;
    let store = FileStore::new(data.path().join("collections/default"));
    let initialized = store.catalog()?.unwrap();
    assert_eq!(
        initialized.lineage.as_ref().unwrap().observation.reason,
        PublicationReason::Initialization
    );
    let first = initialized.receipt.as_ref().unwrap().token;
    assert_eq!(kernel.index()?.generation, first.generation);
    assert_eq!(store.catalog()?.unwrap().receipt.unwrap().token, first);
    fs::write(
        root.path().join("note.md"),
        "# Original\nKeep bounded evidence.\n",
    )?;
    kernel.index()?;
    let added = store.catalog()?.unwrap();
    let state = added.inventory.as_ref().unwrap().files["note.md"]
        .state
        .clone();
    assert_eq!(
        added.lineage.as_ref().unwrap().observation.parent,
        Some(first)
    );
    let added_token = added.receipt.as_ref().unwrap().token;
    assert_eq!(kernel.index()?.generation, added_token.generation);
    assert_eq!(
        store.catalog()?.unwrap().receipt.unwrap().token,
        added_token
    );
    kernel.reindex()?;
    let rebuilt = store.catalog()?.unwrap();
    assert_eq!(
        rebuilt.lineage.as_ref().unwrap().observation.reason,
        PublicationReason::FullReindex
    );
    assert_eq!(
        rebuilt.inventory.as_ref().unwrap().files["note.md"].state,
        state
    );
    fs::write(
        root.path().join("note.md"),
        "# Revised\nKeep bounded evidence.\n",
    )?;
    kernel.index()?;
    let edited = store.catalog()?.unwrap();
    let changed = &edited.inventory.as_ref().unwrap().files["note.md"].state;
    assert_eq!(changed.artifact_id, state.artifact_id);
    assert_eq!(
        changed.stamps.first_generation,
        state.stamps.first_generation
    );
    assert_eq!(changed.stamps.path_generation, state.stamps.path_generation);
    assert!(changed.stamps.content_generation > state.stamps.content_generation);
    assert!(changed.stamps.evidence_generation > state.stamps.evidence_generation);
    assert!(
        added.load_artifact("note.md")?.chunks[0]
            .text
            .contains("Original")
    );
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    assert_eq!(
        reopened.file_info("note.md")?.generation,
        edited.snapshot.generation
    );
    Ok(())
}

#[test]
fn warm_metadata_uses_indexed_rows_and_does_not_decode_inventory_or_payload() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("one.txt"), "Retained metadata.\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let before = kernel.file_info("one.txt")?;
    let directory = data.path().join("collections/default");
    let store = FileStore::new(&directory);
    let catalog = store.catalog()?.unwrap();
    let receipt = catalog.receipt.as_ref().unwrap();
    // Only private test index files are damaged, never source files. The warm
    // catalog lease and exact receipt remain pinned; independent reopening fails.
    fs::write(
        directory
            .join("generations")
            .join(receipt.token.revision.to_string())
            .join("inventory.json"),
        b"invalid",
    )?;
    let object = &catalog.snapshot.artifacts["one.txt"]
        .retained
        .as_ref()
        .unwrap()
        .object;
    fs::write(directory.join("objects/artifacts").join(object), b"invalid")?;
    let warm = kernel.file_info("one.txt")?;
    assert_eq!(warm.content_hash, before.content_hash);
    assert_eq!(warm.indexed_lines, 1);
    assert!(kernel.read_file("one.txt", 1, 1, 1024).is_err());
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    assert!(reopened.file_info("one.txt").is_err());
    Ok(())
}

#[test]
fn concurrent_workers_return_one_pinned_metadata_revision() -> Result<()> {
    use sha2::{Digest, Sha256};
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicBool, Ordering},
    };
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
    let store = FileStore::new(data.path().join("collections/default"));
    let old = store.catalog()?.unwrap();
    let old_revision = old.receipt.as_ref().unwrap().token.revision.to_string();
    let reader = Kernel::open(root.path(), Some(data.path().into()))?;
    let start = Arc::new(Barrier::new(2));
    let done = Arc::new(AtomicBool::new(false));
    let reader_start = start.clone();
    let reader_done = done.clone();
    let thread = std::thread::spawn(move || -> Result<usize> {
        reader_start.wait();
        let mut count = 0;
        loop {
            let info = reader
                .file_info("note.txt")
                .context("concurrent metadata reader")?;
            let index = usize::try_from(info.generation - 1)?;
            let (hash, bytes, lines) = &expected[index];
            assert_eq!(&info.content_hash, hash);
            assert_eq!(info.source_bytes, *bytes);
            assert_eq!(info.indexed_lines, *lines);
            let search = reader
                .search(&airk_core::SearchQuery {
                    text: "Observed".into(),
                    path_prefix: None,
                    limit: 1,
                    max_bytes: 4096,
                })
                .context("concurrent search reader")?;
            assert_eq!(search.hits.len(), 1);
            assert_eq!(
                search.hits[0].citation.content_hash,
                expected[usize::try_from(search.generation - 1)?].0
            );
            count += 1;
            if reader_done.load(Ordering::Acquire) || count >= 2_000 {
                break;
            }
        }
        Ok(count)
    });
    start.wait();
    let writes = (|| -> Result<()> {
        for text in &texts[1..] {
            fs::write(root.path().join("note.txt"), text)?;
            writer.index().context("concurrent publisher")?;
        }
        Ok(())
    })();
    done.store(true, Ordering::Release);
    assert!(thread.join().expect("reader worker panicked")? > 0);
    writes?;
    assert!(writer.prune_indexes(false)?.pinned.contains(&old_revision));
    assert_eq!(old.load_artifact("note.txt")?.chunks[0].text, texts[0]);
    Ok(())
}

#[test]
fn publication_releases_superseded_idle_catalog_and_search_pins() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("one.txt"), "Earlier indexed text.\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    kernel.file_info("one.txt")?;
    kernel.search(&airk_core::SearchQuery {
        text: "Earlier".into(),
        path_prefix: None,
        limit: 1,
        max_bytes: 1024,
    })?;
    let store = FileStore::new(data.path().join("collections/default"));
    let old = store.persistent_revision()?.unwrap().token.unwrap();
    fs::write(root.path().join("one.txt"), "Later indexed text.\n")?;
    kernel.index()?;
    let report = kernel.prune_indexes(false)?;
    assert!(report.candidates.contains(&old.revision.to_string()));
    assert!(!report.pinned.contains(&old.revision.to_string()));
    assert!(store.receipt(&old)?.is_none());
    Ok(())
}
