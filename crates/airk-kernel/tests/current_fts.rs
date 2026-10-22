// SPDX-License-Identifier: Apache-2.0
//! Production FTS5 query semantics and the explicit rebuild boundary.
use airk_core::SearchQuery;
use airk_kernel::Kernel;
use anyhow::Result;
use std::fs;

#[test]
fn warm_search_rejects_changed_postings_even_for_an_empty_result() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(root.path().join("source.md"), "VerifiedNeedle evidence")?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let query = SearchQuery {
        text: "VerifiedNeedle".into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 4096,
    };
    assert_eq!(kernel.search(&query)?.hits.len(), 1);
    assert_eq!(kernel.search(&query)?.hits.len(), 1);
    let database = state.path().join("collections-1.2/default/current.sqlite3");
    let connection = rusqlite::Connection::open(database)?;
    connection.execute(
        "INSERT INTO current_fts(current_fts) VALUES('delete-all')",
        [],
    )?;
    drop(connection);
    assert!(
        kernel.search(&query).is_err(),
        "Corruption must not become a trusted missing match"
    );
    assert!(kernel.diagnose_current(None, None, 16, 1 << 20).is_err());
    Ok(())
}

#[test]
fn fts_preserves_identifiers_unicode_literal_scope_and_incremental_replacement() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::create_dir(root.path().join("%_[x]"))?;
    fs::write(
        root.path().join("%_[x]/first.md"),
        "# Evidence\nCamelCase café λ\n",
    )?;
    fs::write(
        root.path().join("second.md"),
        "# Evidence\nCamelCase other\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    assert_eq!(kernel.status()?.lexical_backend, "sqlite-fts5-current");
    let query = |text: &str, prefix: Option<&str>, limit| SearchQuery {
        text: text.into(),
        path_prefix: prefix.map(str::to_owned),
        limit,
        max_bytes: 4096,
    };
    for text in ["CamelCase", "camel case", "café λ", "\"CamelCase\" OR"] {
        let hits = kernel.search(&query(text, Some("%_[x]/"), 10))?;
        if text.contains("OR") {
            assert!(
                hits.hits.is_empty(),
                "FTS operators must remain literal query terms"
            );
        } else {
            assert_eq!(hits.hits.len(), 1);
            assert_eq!(hits.hits[0].citation.path, "%_[x]/first.md");
        }
    }
    assert!(
        kernel.search(&query("cafe", None, 10))?.hits.is_empty(),
        "do not introduce accent folding"
    );
    assert!(
        kernel
            .search(&query("CamelCase", Some("%_[X]/"), 10))?
            .hits
            .is_empty(),
        "path scope stays case sensitive"
    );
    let bounded = kernel.search(&query("CamelCase", None, 1))?;
    assert_eq!(bounded.hits.len(), 1);
    assert!(bounded.truncated);
    fs::write(
        root.path().join("%_[x]/first.md"),
        "# Replacement\nNewIdentifier\n",
    )?;
    fs::remove_file(root.path().join("second.md"))?;
    kernel.index()?;
    assert!(
        kernel
            .search(&query("CamelCase", None, 10))?
            .hits
            .is_empty()
    );
    assert_eq!(
        kernel.search(&query("NewIdentifier", None, 10))?.hits.len(),
        1
    );
    let database = state.path().join("collections-1.2/default/current.sqlite3");
    let connection = rusqlite::Connection::open(database)?;
    connection.execute(
        "INSERT INTO current_fts(current_fts,rank) VALUES('integrity-check',1)",
        [],
    )?;
    assert_eq!(
        connection.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
        126
    );
    assert!(
        !state
            .path()
            .join("collections-1.2/default/lexical-0")
            .exists()
    );
    Ok(())
}

#[test]
fn explicit_full_rebuild_replaces_tantivy_state_and_preserves_source_and_settings() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(root.path().join("source.md"), "Current source evidence")?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let directory = state.path().join("collections-1.2/default");
    let owner = directory.join("current-owner.json");
    let bytes =
        String::from_utf8(fs::read(&owner)?)?.replacen("\"version\":126", "\"version\":120", 1);
    fs::write(&owner, &bytes)?;
    fs::create_dir(directory.join("lexical-0"))?;
    fs::write(
        directory.join("lexical-0/meta.json"),
        "old derived lexical state",
    )?;
    fs::write(directory.join("unrelated-settings.json"), "preserve me")?;
    drop(kernel);
    let error = match Kernel::open(root.path(), Some(state.path().into())) {
        Ok(_) => panic!("obsolete owner accepted"),
        Err(error) => error,
    };
    assert!(format!("{error:#}").contains("index --full"));
    assert_eq!(fs::read(&owner)?, bytes.as_bytes());
    let rebuilding =
        Kernel::open_collection_for_diagnostics(root.path(), Some(state.path().into()), "default")?;
    assert!(rebuilding.prepare_full_rebuild()?);
    assert!(!rebuilding.vector_status()?.enabled);
    assert!(!rebuilding.prepare_full_rebuild()?);
    rebuilding.reindex()?;
    assert!(!rebuilding.prepare_full_rebuild()?);
    assert_eq!(rebuilding.status()?.files, 1);
    assert!(!rebuilding.vectors_enabled()?);
    assert!(!directory.join("lexical-0").exists());
    assert!(!directory.join("fts5-rebuild.json").exists());
    assert_eq!(
        fs::read_to_string(directory.join("unrelated-settings.json"))?,
        "preserve me"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("source.md"))?,
        "Current source evidence"
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(&owner)?)?["version"],
        126
    );
    // Native scheduled --full records the intent before the worker prepares it.
    fs::write(
        &owner,
        String::from_utf8(fs::read(&owner)?)?.replacen("\"version\":126", "\"version\":120", 1),
    )?;
    rebuilding.scheduled_index(true, false)?;
    assert_eq!(rebuilding.status()?.files, 1);
    assert!(!rebuilding.vectors_enabled()?);
    Ok(())
}

#[test]
fn interrupted_fts_rebuild_resumes_with_or_without_an_owner_header() -> Result<()> {
    #[derive(serde::Serialize)]
    struct Marker {
        version: u32,
        scope: airk_store::current::CurrentScope,
    }
    for owner_survives in [false, true] {
        let root = tempfile::tempdir()?;
        let state = tempfile::tempdir()?;
        fs::write(root.path().join("source.md"), "Recovery source evidence")?;
        let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let directory = state.path().join("collections-1.2/default");
        let owner: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.join("current-owner.json"))?)?;
        fs::write(
            directory.join("fts5-rebuild.json"),
            serde_json::to_vec(&Marker {
                version: 1,
                scope: serde_json::from_value(owner["scope"].clone())?,
            })?,
        )?;
        if owner_survives {
            let owner_path = directory.join("current-owner.json");
            fs::write(
                &owner_path,
                fs::read_to_string(&owner_path)?.replacen("\"version\":126", "\"version\":120", 1),
            )?;
        } else {
            fs::remove_file(directory.join("current-owner.json"))?;
        }
        drop(kernel);
        assert!(Kernel::open(root.path(), Some(state.path().into())).is_err());
        let rebuilding = Kernel::open_collection_for_diagnostics(
            root.path(),
            Some(state.path().into()),
            "default",
        )?;
        rebuilding.scheduled_index(true, false)?;
        assert_eq!(rebuilding.status()?.files, 1);
        assert!(!directory.join("fts5-rebuild.json").exists());
    }
    Ok(())
}

#[test]
fn compatible_rebuild_preparation_does_not_take_the_reader_gate() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(root.path().join("file.txt"), "fixture")?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let directory = state.path().join("collections-1.2/default");
    let owner: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("current-owner.json"))?)?;
    let scope = serde_json::from_value(owner["scope"].clone())?;
    let store = airk_store::FileStore::new(directory);
    let reader = store.current_read(&scope)?;
    assert!(!kernel.prepare_full_rebuild()?);
    assert_eq!(reader.token()?.sequence, kernel.status()?.generation);
    Ok(())
}

#[test]
fn fts_preparation_grows_capacity_from_authenticated_input_sizes() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(root.path().join("small.txt"), "initial")?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let directory = state.path().join("collections-1.2/default");
    let capacity = || -> Result<u64> {
        Ok(serde_json::from_slice::<serde_json::Value>(&fs::read(
            directory.join("current-owner.json"),
        )?)?["capacity"]
            .as_u64()
            .unwrap())
    };
    let before = capacity()?;
    let source = (0..16000).map(|i| format!("CapacityNeedle item{i} original evidence with enough retained text to grow admission.\n")).collect::<String>();
    fs::write(root.path().join("large.txt"), source)?;
    kernel.index()?;
    assert!(capacity()? > before);
    assert!(
        !kernel
            .search(&SearchQuery {
                text: "CapacityNeedle".into(),
                path_prefix: None,
                limit: 1,
                max_bytes: 1024
            })?
            .hits
            .is_empty()
    );
    Ok(())
}
