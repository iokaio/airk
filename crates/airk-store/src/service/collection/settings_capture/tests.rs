// SPDX-License-Identifier: Apache-2.0
use super::*;

fn fixture() -> Result<(tempfile::TempDir, Collection, FileStore)> {
    let owner = tempfile::tempdir()?;
    let data = owner.path().join("data");
    let source = owner.path().join("source");
    fs::create_dir(&data)?;
    fs::create_dir(&source)?;
    fs::write(source.join("original.txt"), b"Original recovery source.")?;
    let source = source.canonicalize()?;
    let collection = Collection::initialize(
        &data,
        source.to_str().context("non-UTF-8 fixture")?,
        "fixture",
        Incarnation::from_bytes([17; 16]),
    )?;
    let store = collection.selected_store()?;
    drop(store.lock()?);
    Ok((owner, collection, store))
}

fn tree(path: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    fn visit(base: &Path, path: &Path, result: &mut Vec<(PathBuf, Vec<u8>)>) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                visit(base, &entry.path(), result)?;
            } else {
                result.push((
                    entry.path().strip_prefix(base)?.to_path_buf(),
                    fs::read(entry.path())?,
                ));
            }
        }
        Ok(())
    }
    let mut result = Vec::new();
    visit(path, path, &mut result)?;
    result.sort();
    Ok(result)
}

#[test]
fn capture_saved_settings_retains_exact_bytes_absence_identity_and_source() -> Result<()> {
    let (fixture, collection, store) = fixture()?;
    let root = &collection.owner.root;
    let roots = serde_json::to_vec_pretty(&serde_json::json!({
        "schema": 1, "owner_root": root, "roots": {}
    }))?;
    let vectors = serde_json::to_vec_pretty(&serde_json::json!({
        "version": 1, "owner_root": root,
        "policy": {"enabled": false, "selection": {"opaque": "preserved exactly"}}
    }))?;
    fs::write(store.directory().join("roots.json"), &roots)?;
    fs::write(store.directory().join("vector-policy.json"), &vectors)?;
    let before = tree(fixture.path())?;
    let old_digest = inspect_settings_digest(&store, root)?;
    let captured = collection.capture_saved_settings()?;
    assert_eq!(captured.owner_root(), root);
    assert_eq!(captured.collection(), "fixture");
    assert_eq!(captured.owner(), collection.binding);
    assert_eq!(captured.incarnation(), Incarnation::from_bytes([17; 16]));
    let files = captured.files().collect::<Vec<_>>();
    assert_eq!(files.len(), 5);
    assert_eq!(files[0], ("roots.json", Some(roots.as_slice())));
    assert_eq!(files[1], ("extraction-policy.json", None));
    assert_eq!(files[2], ("vector-policy.json", Some(vectors.as_slice())));
    assert_eq!(files[3], ("compression-policy.json", None));
    assert_eq!(files[4], ("scan-limits.json", None));
    assert_eq!(
        captured.digest(),
        collection.capture_saved_settings()?.digest()
    );
    assert_eq!(old_digest, inspect_settings_digest(&store, root)?);
    assert_eq!(before, tree(fixture.path())?);
    // Explicit presence remains distinct even when its effective policy equals
    // the default that an owner would use for an absent configuration.
    fs::remove_file(store.directory().join("roots.json"))?;
    assert_ne!(
        captured.digest(),
        collection.capture_saved_settings()?.digest()
    );
    Ok(())
}

#[test]
fn capture_saved_settings_refuses_foreign_unsupported_and_malformed_envelopes() -> Result<()> {
    let (_fixture, collection, store) = fixture()?;
    let path = store.directory().join("vector-policy.json");
    for value in [
        serde_json::json!({"version":1,"owner_root":"foreign-root","policy":{}}),
        serde_json::json!({"version":2,"owner_root":collection.owner.root,"policy":{}}),
        serde_json::json!({"version":1,"owner_root":collection.owner.root,"policy":[],"future":true}),
    ] {
        let bytes = serde_json::to_vec(&value)?;
        fs::write(&path, &bytes)?;
        assert!(collection.capture_saved_settings().is_err());
        assert_eq!(fs::read(&path)?, bytes);
    }
    fs::write(&path, b"{")?;
    assert!(collection.capture_saved_settings().is_err());
    assert_eq!(fs::read(&path)?, b"{");
    fs::remove_file(&path)?;
    fs::write(store.directory().join("future-policy.json"), b"{}")?;
    let error = collection
        .capture_saved_settings()
        .err()
        .context("unknown policy accepted")?;
    assert!(
        error
            .to_string()
            .contains("unsupported saved settings file")
    );
    assert_eq!(
        fs::read(store.directory().join("future-policy.json"))?,
        b"{}"
    );
    Ok(())
}

#[test]
fn capture_saved_settings_preserves_actual_compression_owner_payload() -> Result<()> {
    let (_fixture, collection, store) = fixture()?;
    let quota = Quota::open_service(Path::new(&collection.owner.data_root))?;
    let writer = store.lock()?;
    store.configure_compression(
        &writer,
        &quota,
        &collection.owner.root,
        airk_core::compression::CompressionMode::Automatic,
    )?;
    drop(writer);
    let original = fs::read(store.directory().join("compression-policy.json"))?;
    let captured = collection.capture_saved_settings()?;
    assert_eq!(
        captured
            .files()
            .find(|(name, _)| *name == "compression-policy.json"),
        Some(("compression-policy.json", Some(original.as_slice())))
    );
    Ok(())
}

#[test]
fn capture_saved_settings_refuses_oversized_non_file_and_live_writer_without_changes() -> Result<()>
{
    let (fixture, collection, store) = fixture()?;
    let path = store.directory().join("roots.json");
    fs::write(&path, vec![b' '; MAX_SETTING_BYTES as usize + 1])?;
    let before = tree(fixture.path())?;
    assert!(collection.capture_saved_settings().is_err());
    assert_eq!(before, tree(fixture.path())?);
    fs::remove_file(&path)?;
    fs::create_dir(&path)?;
    assert!(collection.capture_saved_settings().is_err());
    fs::remove_dir(&path)?;
    let writer = store.lock()?;
    assert!(collection.capture_saved_settings().is_err());
    drop(writer);
    collection.capture_saved_settings()?;
    Ok(())
}

#[test]
fn capture_saved_settings_requires_existing_selection_and_advisory() -> Result<()> {
    let (_fixture, collection, store) = fixture()?;
    let managed = store.managed_path()?;
    let advisory = managed.root.join("collections-1.4/.writer-locks/fixture");
    let bytes = fs::read(&advisory)?;
    fs::remove_file(&advisory)?;
    assert!(collection.capture_saved_settings().is_err());
    assert!(
        !advisory.exists(),
        "read-only capture created a missing advisory"
    );
    fs::write(&advisory, bytes)?;
    let active = collection.directory.join("active.json");
    let bytes = fs::read(&active)?;
    fs::remove_file(&active)?;
    assert!(collection.capture_saved_settings().is_err());
    assert!(
        !active.exists(),
        "read-only capture created a missing manifest"
    );
    fs::write(active, bytes)?;
    collection.capture_saved_settings()?;
    Ok(())
}
