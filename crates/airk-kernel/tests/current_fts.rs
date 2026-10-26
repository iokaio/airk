// SPDX-License-Identifier: Apache-2.0
//! Production native lexical query semantics and the explicit rebuild boundary.
use airk_core::SearchQuery;
use airk_kernel::Kernel;
use anyhow::Result;
use std::fs;

#[test]
fn lexical_preserves_identifiers_unicode_literal_scope_and_incremental_replacement() -> Result<()> {
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
    assert_eq!(kernel.status()?.lexical_backend, "redb-paged-postings");
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
                "Query operators must remain literal query terms"
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
    assert!(
        !state
            .path()
            .join("collections-1.2/default/lexical-0")
            .exists()
    );
    Ok(())
}

#[test]
fn unsupported_ownership_requires_fresh_setup_and_preserves_every_file() -> Result<()> {
    for old_version in [120, 126, 140] {
        let root = tempfile::tempdir()?;
        let state = tempfile::tempdir()?;
        fs::write(root.path().join("source.md"), "Preserve original evidence")?;
        let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let directory = state.path().join("collections-1.2/default");
        let owner_path = directory.join("current-owner.json");
        let bytes = String::from_utf8(fs::read(&owner_path)?)?
            .replacen("\"version\":150", &format!("\"version\":{old_version}"), 1)
            .into_bytes();
        fs::write(&owner_path, &bytes)?;
        fs::write(directory.join("unrelated-settings.json"), "preserve me")?;
        let error = kernel.prepare_full_rebuild().unwrap_err();
        assert!(format!("{error:#}").contains("fresh setup"), "{error:#}");
        assert_eq!(fs::read(owner_path)?, bytes);
        assert_eq!(
            fs::read_to_string(directory.join("unrelated-settings.json"))?,
            "preserve me"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("source.md"))?,
            "Preserve original evidence"
        );
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
fn native_publication_grows_capacity_from_authenticated_inputs() -> Result<()> {
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
