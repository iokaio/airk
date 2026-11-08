// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::fs;
use std::time::Duration;
fn limits() -> CaptureLimits {
    CaptureLimits {
        max_files: 64,
        max_bytes: 1 << 20,
        max_file_bytes: 1 << 18,
        max_depth: 8,
        max_snapshot_bytes: 1 << 20,
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

#[test]
fn exact_copy_preserves_original_bytes_and_refuses_foreign_or_changed_resume() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    let target = owner.path().join("copy");
    fs::create_dir(&source)?;
    fs::create_dir(&target)?;
    fs::create_dir(source.join("objects"))?;
    fs::write(
        source.join("index.sqlite3"),
        b"original saved revision bytes",
    )?;
    fs::write(
        source.join("objects/payload"),
        b"original extracted bytes\n",
    )?;
    let before = capture_tree(&source, limits(), deadline())?;
    copy_tree(&source, &target, &before, limits(), deadline(), &mut || {
        Ok(())
    })?;
    assert_eq!(capture_tree(&source, limits(), deadline())?, before);
    assert_eq!(capture_tree(&target, limits(), deadline())?, before);
    fs::write(target.join("unrelated-user-file"), b"keep")?;
    assert!(
        copy_tree(&source, &target, &before, limits(), deadline(), &mut || Ok(
            ()
        ))
        .is_err()
    );
    assert_eq!(fs::read(target.join("unrelated-user-file"))?, b"keep");
    assert_eq!(capture_tree(&source, limits(), deadline())?, before);
    Ok(())
}

#[test]
fn interrupted_known_copy_part_resumes_but_crossed_prefix_is_preserved() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    let target = owner.path().join("target");
    fs::create_dir(&source)?;
    fs::create_dir(&target)?;
    fs::write(source.join("saved"), b"saved full predecessor bytes")?;
    let inventory = capture_tree(&source, limits(), deadline())?;
    let parts = target.join(PARTS);
    fs::create_dir(&parts)?;
    let part = parts.join(inventory.files["saved"].sha256.to_string());
    fs::write(&part, b"saved full")?;
    copy_tree(
        &source,
        &target,
        &inventory,
        limits(),
        deadline(),
        &mut || Ok(()),
    )?;
    assert!(!parts.exists());
    assert_eq!(capture_tree(&target, limits(), deadline())?, inventory);
    fs::create_dir(&parts)?;
    fs::write(&part, b"foreign")?;
    assert!(
        copy_tree(
            &source,
            &target,
            &inventory,
            limits(),
            deadline(),
            &mut || Ok(())
        )
        .is_err()
    );
    assert_eq!(fs::read(&part)?, b"foreign");
    assert_eq!(
        fs::read(target.join("saved"))?,
        b"saved full predecessor bytes"
    );
    Ok(())
}

#[test]
fn lost_exclusion_or_exceeded_capture_budget_never_changes_original() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    let target = owner.path().join("target");
    fs::create_dir(&source)?;
    fs::create_dir(&target)?;
    fs::write(source.join("saved"), b"saved")?;
    let inventory = capture_tree(&source, limits(), deadline())?;
    assert!(
        copy_tree(
            &source,
            &target,
            &inventory,
            limits(),
            deadline(),
            &mut || anyhow::bail!("native owner changed")
        )
        .is_err()
    );
    assert_eq!(fs::read_dir(&target)?.count(), 0);
    assert!(
        capture_tree(
            &source,
            CaptureLimits {
                max_bytes: 1,
                ..limits()
            },
            deadline()
        )
        .is_err()
    );
    assert_eq!(capture_tree(&source, limits(), deadline())?, inventory);
    Ok(())
}

#[test]
fn changed_partial_identity_is_preserved_before_any_unlink() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    let target = owner.path().join("target");
    fs::create_dir(&source)?;
    fs::create_dir(&target)?;
    fs::write(source.join("saved"), b"original saved predecessor bytes")?;
    let inventory = capture_tree(&source, limits(), deadline())?;
    let parts = target.join(PARTS);
    fs::create_dir(&parts)?;
    let part = parts.join(inventory.files["saved"].sha256.to_string());
    fs::write(&part, b"original")?;
    let retained = owner.path().join("retained-original-part");
    let mut calls = 0;
    let result = copy_tree(
        &source,
        &target,
        &inventory,
        limits(),
        deadline(),
        &mut || {
            calls += 1;
            if calls == 2 {
                fs::rename(&part, &retained)?;
                fs::write(&part, b"foreign replacement")?;
            }
            Ok(())
        },
    );
    assert!(result.is_err());
    assert_eq!(fs::read(&part)?, b"foreign replacement");
    assert_eq!(fs::read(&retained)?, b"original");
    assert_eq!(capture_tree(&source, limits(), deadline())?, inventory);
    Ok(())
}

#[test]
fn empty_directory_removal_refuses_crossed_identity_or_nonempty_children() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let pins = PinnedRoot::open(owner.path())?;
    let first = pins.root().create_dir(OsStr::new("first"))?;
    let second = pins.root().create_dir(OsStr::new("second"))?;
    assert!(
        pins.root()
            .unlink_empty_dir_if_identity(OsStr::new("second"), &first)
            .is_err()
    );
    let mut content = second.open_file(OsStr::new("unknown"), SetupFileMode::CreateNew)?;
    content.write_all(b"preserve")?;
    content.sync_all()?;
    assert!(
        pins.root()
            .unlink_empty_dir_if_identity(OsStr::new("second"), &second)
            .is_err()
    );
    assert_eq!(fs::read(owner.path().join("second/unknown"))?, b"preserve");
    pins.root()
        .unlink_empty_dir_if_identity(OsStr::new("first"), &first)?;
    drop(first);
    assert!(!owner.path().join("first").exists());
    Ok(())
}

#[test]
fn validation_preimage_is_checked_before_mutable_reader_metadata() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let validation = owner.path().join("working-copy");
    fs::create_dir(&validation)?;
    fs::write(validation.join("unrelated"), b"preserve unknown bytes")?;
    let before = capture_tree(&validation, limits(), deadline())?;
    let mut crossed = before.clone();
    crossed.bytes += 1;
    let result = VerifiedLegacyArchive::open_validation_copy(
        &validation,
        LegacyScope {
            root: "unread-original-source".into(),
            collection: "saved".into(),
            roots: BTreeMap::new(),
        },
        &crossed,
        limits(),
        deadline(),
    );
    assert!(result.is_err());
    assert_eq!(capture_tree(&validation, limits(), deadline())?, before);
    Ok(())
}

#[test]
fn actual_saved_writer_lock_refuses_competitor_and_never_creates_missing_advisory() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    let collection = root.join("saved");
    fs::create_dir(&collection)?;
    let before = capture_tree(&root, limits(), deadline())?;
    assert!(LegacyWriterExclusion::acquire_existing(&collection).is_err());
    assert_eq!(capture_tree(&root, limits(), deadline())?, before);
    let locks = root.join(".writer-locks");
    fs::create_dir(&locks)?;
    assert!(LegacyWriterExclusion::acquire_existing(&collection).is_err());
    assert!(!locks.join("saved").exists());
    fs::write(locks.join("saved"), b"original advisory")?;
    let before = capture_tree(&root, limits(), deadline())?;
    let held = LegacyWriterExclusion::acquire_existing(&collection)?;
    assert!(LegacyWriterExclusion::acquire_existing(&collection).is_err());
    held.revalidate()?;
    drop(held);
    let replacement = LegacyWriterExclusion::acquire_existing(&collection)?;
    replacement.revalidate()?;
    drop(replacement);
    assert_eq!(capture_tree(&root, limits(), deadline())?, before);
    Ok(())
}

#[test]
fn legacy_collection_identity_is_physical_and_preserves_original_markers() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    fs::create_dir(root.join(".writer-locks"))?;
    for name in ["first", "second"] {
        fs::create_dir(root.join(name))?;
        fs::write(root.join(".writer-locks").join(name), b"original advisory")?;
        fs::write(
            root.join(name).join("writer.lock"),
            b"original historical marker",
        )?;
        fs::write(
            root.join(name).join("saved.bin"),
            b"identical saved evidence",
        )?;
    }
    let before = capture_tree(&root, limits(), deadline())?;
    let first = LegacyWriterExclusion::acquire_existing(&root.join("first"))?;
    let first_identity = first.collection_identity()?;
    let second = LegacyWriterExclusion::acquire_existing(&root.join("second"))?;
    assert_ne!(first_identity, second.collection_identity()?);
    assert_eq!(
        fs::read(root.join("first/writer.lock"))?,
        b"original historical marker"
    );
    drop(first);
    drop(second);
    let reopened = LegacyWriterExclusion::acquire_existing(&root.join("first"))?;
    assert_eq!(first_identity, reopened.collection_identity()?);
    drop(reopened);
    assert_eq!(capture_tree(&root, limits(), deadline())?, before);
    Ok(())
}

#[test]
fn legacy_writer_exclusion_refuses_replaced_collection_with_unchanged_advisory() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    let collection = root.join("saved");
    fs::create_dir(&collection)?;
    fs::create_dir(root.join(".writer-locks"))?;
    fs::write(root.join(".writer-locks/saved"), b"original advisory")?;
    fs::write(collection.join("writer.lock"), b"original marker")?;
    fs::write(collection.join("saved.bin"), b"original saved evidence")?;
    let held = LegacyWriterExclusion::acquire_existing(&collection)?;
    let original_identity = held.collection_identity()?;
    let displaced = root.join("displaced-original");
    fs::rename(&collection, &displaced)?;
    fs::create_dir(&collection)?;
    fs::write(collection.join("saved.bin"), b"crossed replacement")?;
    assert!(held.revalidate().is_err());
    assert!(held.collection_identity().is_err());
    assert_eq!(fs::read(displaced.join("writer.lock"))?, b"original marker");
    assert_eq!(
        fs::read(displaced.join("saved.bin"))?,
        b"original saved evidence"
    );
    assert_eq!(
        fs::read(collection.join("saved.bin"))?,
        b"crossed replacement"
    );
    drop(held);
    let replacement = LegacyWriterExclusion::acquire_existing(&collection)?;
    assert_ne!(original_identity, replacement.collection_identity()?);
    Ok(())
}

#[cfg(unix)]
#[test]
fn redirected_saved_index_entry_is_not_followed() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    fs::create_dir(&source)?;
    let other = owner.path().join("other");
    fs::write(&other, b"foreign source")?;
    std::os::unix::fs::symlink(&other, source.join("saved"))?;
    assert!(capture_tree(&source, limits(), deadline()).is_err());
    assert_eq!(fs::read(other)?, b"foreign source");
    Ok(())
}

/// A real compact catalog publication copied without the namespace's OS locks.
/// This is a format/reader component fixture, not an authentic released binary.
#[test]
fn legacy_adoption_copied_catalog_recreates_only_scratch_reader_lease() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let original = owner.path().join("original/collections-1.2/saved");
    let archive = owner.path().join("archive/saved");
    let validation = owner.path().join("validation/collections-1.2/saved");
    fs::create_dir_all(&original)?;
    fs::create_dir_all(&archive)?;
    fs::create_dir_all(&validation)?;
    let store = FileStore::new(&original);
    let mut snapshot = Snapshot::empty("missing-original-source".into());
    snapshot.collection = "saved".into();
    snapshot.generation = 1;
    snapshot.storage_format = 1;
    snapshot.persistent_indexes = true;
    let inventory_bytes = serde_json::to_vec(&snapshot)?;
    let engine = "legacy-adoption-component-fixture";
    let mut digest = Sha256::new();
    digest.update(engine.as_bytes());
    digest.update(&inventory_bytes);
    let revision = format!("{:x}", digest.finalize());
    let generation = original.join("generations").join(&revision);
    fs::create_dir_all(&generation)?;
    store.prepare_catalog(&generation, &snapshot, &revision, &snapshot, None, 1 << 20)?;
    fs::write(generation.join("inventory.json"), inventory_bytes)?;
    fs::write(
        generation.join("manifest.json"),
        serde_json::to_vec(&serde_json::json!({"engine":engine}))?,
    )?;
    store.publish_catalog(&snapshot, &revision, None)?;
    let before = capture_tree(&original, limits(), deadline())?;
    copy_tree(
        &original,
        &archive,
        &before,
        limits(),
        deadline(),
        &mut || Ok(()),
    )?;
    copy_tree(
        &archive,
        &validation,
        &before,
        limits(),
        deadline(),
        &mut || Ok(()),
    )?;
    let fresh = |name: &str| -> Result<PathBuf> {
        let copy = owner.path().join(name).join("collections-1.2/saved");
        fs::create_dir_all(&copy)?;
        copy_tree(&archive, &copy, &before, limits(), deadline(), &mut || {
            Ok(())
        })?;
        Ok(copy)
    };
    let probe = fresh("ordinary-probe")?;
    assert!(
        FileStore::new(&probe).catalog_bounded(1 << 20).is_err(),
        "ordinary reader must not synthesize missing locks"
    );
    let lease_path = crate::blobs::reader_lease_path(&validation, &revision)?;
    assert!(!lease_path.exists());
    let first = VerifiedLegacyArchive::open_discovered_validation_copy(
        &validation,
        &before,
        limits(),
        deadline(),
    )
    .context("first adoption read of pristine validation copy")?;
    assert_eq!(first.snapshot().generation, 1);
    assert_eq!(first.identity().header.as_ref().unwrap().revision, revision);
    let identity = serde_json::to_vec(first.identity())?;
    let scope = first.scope().clone();
    drop(first);
    assert_eq!(fs::metadata(&lease_path)?.len(), 0);
    // READ_ONLY SQLite may still create coordination files in writable scratch.
    // These bytes are never removed from or ignored by the sealed inventory.
    let after_read = capture_tree(&validation, limits(), deadline())?;
    for (name, saved) in &before.files {
        assert_eq!(
            after_read.files.get(name),
            Some(saved),
            "reader changed original file {name}"
        );
    }
    let added = after_read
        .files
        .keys()
        .filter(|name| !before.files.contains_key(*name))
        .map(String::as_str)
        .collect::<Vec<_>>();
    assert_eq!(
        added,
        vec!["index.sqlite3-shm", "index.sqlite3-wal"],
        "exact scratch reader delta: {after_read:?}"
    );
    assert_eq!(after_read.files["index.sqlite3-shm"].bytes, 32768);
    assert_eq!(after_read.files["index.sqlite3-wal"].bytes, 0);
    assert!(
        VerifiedLegacyArchive::open_discovered_validation_copy(
            &validation,
            &before,
            limits(),
            deadline()
        )
        .is_err(),
        "changed scratch must not be relabeled as the sealed preimage"
    );
    let reopened = fresh("conversion")?;
    let second = VerifiedLegacyArchive::open_validation_copy(
        &reopened,
        scope,
        &before,
        limits(),
        deadline(),
    )
    .context("conversion read of fresh exact archive copy")?;
    assert_eq!(serde_json::to_vec(second.identity())?, identity);
    drop(second);
    assert_eq!(capture_tree(&original, limits(), deadline())?, before);
    assert_eq!(capture_tree(&archive, limits(), deadline())?, before);
    assert!(!crate::blobs::reader_lease_path(&original, &revision)?.exists());
    assert!(!crate::blobs::reader_lease_path(&archive, &revision)?.exists());
    let negative = fresh("nonempty-lease")?;
    let bad_lease = crate::blobs::reader_lease_path(&negative, &revision)?;
    fs::create_dir_all(bad_lease.parent().unwrap())?;
    fs::write(&bad_lease, b"unknown preserved lease")?;
    let error = match VerifiedLegacyArchive::open_discovered_validation_copy(
        &negative,
        &before,
        limits(),
        deadline(),
    ) {
        Ok(_) => anyhow::bail!("nonempty lease accepted"),
        Err(error) => error,
    };
    assert!(format!("{error:#}").contains("unknown nonempty validation reader lease preserved"));
    assert_eq!(fs::read(&bad_lease)?, b"unknown preserved lease");
    #[cfg(unix)]
    {
        let linked = fresh("linked-lease")?;
        let linked_lease = crate::blobs::reader_lease_path(&linked, &revision)?;
        fs::create_dir_all(linked_lease.parent().unwrap())?;
        std::os::unix::fs::symlink(original.join("index.sqlite3"), &linked_lease)?;
        assert!(
            VerifiedLegacyArchive::open_discovered_validation_copy(
                &linked,
                &before,
                limits(),
                deadline()
            )
            .is_err()
        );
        assert_eq!(capture_tree(&original, limits(), deadline())?, before);
    }
    let malformed = owner.path().join("malformed/collections-1.2/saved");
    fs::create_dir_all(&malformed)?;
    copy_tree(
        &archive,
        &malformed,
        &before,
        limits(),
        deadline(),
        &mut || Ok(()),
    )?;
    let db = rusqlite::Connection::open(malformed.join("index.sqlite3"))?;
    db.execute("UPDATE index_components SET payload='{}' WHERE id=1", [])?;
    drop(db);
    let malformed_inventory = capture_tree(&malformed, limits(), deadline())?;
    assert!(
        VerifiedLegacyArchive::open_discovered_validation_copy(
            &malformed,
            &malformed_inventory,
            limits(),
            deadline()
        )
        .is_err()
    );
    assert!(!malformed.parent().unwrap().join(".reader-locks").exists());
    Ok(())
}
