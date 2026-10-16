// SPDX-License-Identifier: Apache-2.0
use airk_core::{ArtifactSource, SearchQuery, Snapshot, SnapshotStore};
use airk_kernel::Kernel;
use airk_store::{FileStore, MemoryStore};
use std::fs;

#[cfg(windows)]
#[test]
fn long_existing_and_new_windows_data_roots_index_reopen_and_query_without_source_changes()
-> anyhow::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let root = tempfile::tempdir()?;
    let storage = tempfile::tempdir()?;
    let content = "def long_path_fixture():\n    return True\n";
    fs::write(root.path().join("fixture.py"), content)?;
    for present in [false, true] {
        let directory = storage
            .path()
            .join("a".repeat(90))
            .join("b".repeat(90))
            .join(if present {
                "existing-".repeat(12)
            } else {
                "not-created-".repeat(9)
            });
        assert!(directory.as_os_str().encode_wide().count() > 260);
        if present {
            fs::create_dir_all(&directory)?;
        }
        let kernel = Kernel::open(root.path(), Some(directory.clone()))?;
        assert_eq!(
            directory.exists(),
            present,
            "opening a new data root must remain read-only"
        );
        assert!(kernel.status().is_err());
        assert_eq!(
            directory.exists(),
            present,
            "status must not create a data directory"
        );
        let indexed = kernel.index()?;
        assert_eq!(indexed.indexed, 1);
        drop(kernel);
        let reopened = Kernel::open(
            root.path(),
            Some(directory.join("unused").join("..").join(".")),
        )?;
        assert_eq!(reopened.status()?.generation, indexed.generation);
        assert_eq!(reopened.search(&query("long_path_fixture"))?.hits.len(), 1);
        assert_eq!(
            reopened.outline("fixture.py", 10, 4096)?.facts[0].name,
            "long_path_fixture"
        );
        assert_eq!(reopened.index()?.reused, 1);
        assert_eq!(fs::read_to_string(root.path().join("fixture.py"))?, content);
    }
    Ok(())
}

#[test]
fn full_reindex_reextracts_without_resetting_generation_or_destroying_prior_state()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(root.path().join("a.rs"), "fn original_fixture() {}\n")?;
    let kernel = Kernel::open(root.path(), Some(state.path().to_path_buf()))?;
    let first = kernel.index()?;
    assert_eq!(kernel.index()?.reused, 1);
    let full = kernel.reindex()?;
    assert_eq!(full.indexed, 1);
    assert_eq!(full.reused, 0);
    assert_eq!(full.generation, first.generation + 1);
    let lock = state.path().join("collections/default/writer.lock");
    fs::write(&lock, "fixture writer lock")?;
    assert!(kernel.reindex().is_err());
    assert_eq!(kernel.status()?.generation, full.generation);
    assert_eq!(kernel.status()?.files, 1);
    fs::remove_file(lock)?;
    fs::remove_file(root.path().join("a.rs"))?;
    fs::write(root.path().join("b.rs"), "fn replacement_fixture() {}\n")?;
    assert_eq!(kernel.reindex()?.removed, 1);
    Ok(())
}

fn query(text: &str) -> SearchQuery {
    SearchQuery {
        text: text.into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 8192,
    }
}

#[test]
fn incremental_index_reopen_change_delete_and_cache_refresh() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    fs::write(root.path().join("auth.custom"), "fn validateToken() {}\n")?;
    let kernel = Kernel::open(root.path(), None)?;
    assert_eq!(kernel.index()?.indexed, 1);
    let first = kernel.search(&query("validateToken"))?;
    assert_eq!(first.hits.len(), 1);
    assert_eq!(first.hits[0].citation.start_line, 1);
    let unchanged = kernel.index()?;
    assert_eq!(unchanged.reused, 1);
    assert_eq!(unchanged.indexed, 0);
    assert_eq!(unchanged.generation, first.generation);
    let reopened = Kernel::open(root.path(), None)?;
    fs::write(
        root.path().join("auth.custom"),
        "fn authorizeSession() {}\n",
    )?;
    assert_eq!(reopened.index()?.indexed, 1);
    assert!(kernel.search(&query("validateToken"))?.hits.is_empty());
    let changed = kernel.search(&query("authorizeSession"))?;
    assert_ne!(
        changed.hits[0].citation.content_hash,
        first.hits[0].citation.content_hash
    );
    fs::remove_file(root.path().join("auth.custom"))?;
    assert_eq!(reopened.index()?.removed, 1);
    assert!(kernel.search(&query("authorizeSession"))?.hits.is_empty());
    Ok(())
}

#[test]
fn ignore_secrets_binary_and_external_index_directory() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    fs::write(root.path().join(".gitignore"), "temp/\n")?;
    fs::write(root.path().join(".airkignore"), "private.txt\n")?;
    fs::create_dir(root.path().join("temp"))?;
    fs::write(root.path().join("temp/reference.txt"), "sensitive needle")?;
    fs::write(root.path().join(".env"), "SECRET=needle")?;
    fs::write(root.path().join("private.txt"), "needle")?;
    fs::write(root.path().join("good.unknown"), "public needle")?;
    fs::write(root.path().join("binary.pdf"), [0, 159, 255])?;
    let kernel = Kernel::open(root.path(), None)?;
    let report = kernel.index()?;
    assert!(report.skipped.iter().any(|s| s.path == "binary.pdf"));
    let paths = kernel.list_files("", 100, 8192)?.paths;
    assert!(paths.contains(&"good.unknown".into()));
    assert!(!paths.iter().any(|p| p.starts_with(".airk/")
        || p.starts_with("temp/")
        || p == "private.txt"
        || p == ".env"));
    assert_eq!(kernel.search(&query("needle"))?.hits.len(), 1);
    Ok(())
}

#[test]
fn budgets_unicode_line_ranges_and_inventory_only_reads() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    fs::write(
        root.path().join("notes.md"),
        "# 知識\nneedle ééé\nthird line\n",
    )?;
    let kernel = Kernel::open(root.path(), None)?;
    kernel.index()?;
    let read = kernel.read_file("notes.md", 2, 2, 8192)?;
    assert_eq!(read.hits[0].text, "needle ééé\n");
    assert!(kernel.read_file("../notes.md", 1, 1, 8192).is_err());
    assert!(kernel.read_file("notes.md", 0, 2, 8192).is_err());
    assert!(kernel.read_file("notes.md", 1, 90, 8192).is_err());
    let limited = kernel.read_file("notes.md", 2, 3, 8)?;
    assert!(limited.truncated);
    assert!(limited.hits[0].text.len() <= 8);
    let mut q = query("needle");
    q.max_bytes = 5;
    let response = kernel.search(&q)?;
    assert!(response.truncated);
    assert!(response.hits.iter().map(|h| h.text.len()).sum::<usize>() <= 5);
    q.limit = 101;
    assert!(kernel.search(&q).is_err());
    assert!(kernel.search(&query("___")).is_err());
    assert!(kernel.list_files("", 0, 8192).is_err());
    Ok(())
}

#[test]
fn path_scope_and_hit_count_are_enforced() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    fs::create_dir(root.path().join("src"))?;
    fs::write(root.path().join("src/a.rs"), "needle\n")?;
    fs::write(root.path().join("b.md"), "needle\n")?;
    let kernel = Kernel::open(root.path(), None)?;
    kernel.index()?;
    let mut q = query("needle");
    q.limit = 1;
    assert!(kernel.search(&q)?.truncated);
    q.path_prefix = Some("src/".into());
    let scoped = kernel.search(&q)?;
    assert_eq!(scoped.hits[0].citation.path, "src/a.rs");
    assert!(!scoped.truncated);
    Ok(())
}

#[test]
fn writer_lock_schema_and_workspace_identity_fail_closed() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let store = FileStore::new(state.path().join("collections/default"));
    let lock = store.lock()?;
    assert!(store.lock().is_err());
    drop(lock);
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.initialize()?;
    let other = tempfile::tempdir()?;
    assert!(
        Kernel::open(other.path(), Some(state.path().into()))?
            .status()
            .is_err()
    );
    let mut snapshot = store.load()?.expect("initialized");
    snapshot.schema_version = 999;
    let head = store.expected_head()?;
    assert!(store.save(&snapshot).is_err());
    assert_eq!(store.expected_head()?, head);
    assert!(kernel.status().is_ok());
    // Corrupt an independent original legacy fixture, never rewrite a v2 head.
    let legacy_state = tempfile::tempdir()?;
    let legacy_directory = legacy_state.path().join("collections/default");
    fs::create_dir_all(&legacy_directory)?;
    let mut legacy = Snapshot::empty(root.path().canonicalize()?.to_string_lossy().into_owned());
    legacy.schema_version = 999;
    fs::write(
        legacy_directory.join("snapshot.json"),
        serde_json::to_vec(&legacy)?,
    )?;
    assert!(
        Kernel::open(root.path(), Some(legacy_state.path().into()))?
            .status()
            .is_err()
    );
    assert!(Kernel::open(root.path(), Some(root.path().into())).is_err());
    Ok(())
}

#[test]
fn oversized_files_are_reported_and_no_longer_searchable() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let file = root.path().join("large.txt");
    fs::write(&file, "needle")?;
    let kernel = Kernel::open(root.path(), None)?;
    kernel.index()?;
    fs::write(&file, vec![b'x'; airk_ingest::MAX_FILE_BYTES + 1])?;
    let report = kernel.index()?;
    assert_eq!(report.removed, 1);
    assert_eq!(report.skipped.len(), 1);
    assert!(kernel.search(&query("needle"))?.hits.is_empty());
    Ok(())
}

#[test]
fn memory_store_implements_same_snapshot_contract() -> anyhow::Result<()> {
    let store = MemoryStore::default();
    assert!(store.load()?.is_none());
    store.save(&Snapshot::empty("fictional-workspace".into()))?;
    assert_eq!(store.load()?.expect("saved").root, "fictional-workspace");
    Ok(())
}

#[test]
fn collections_share_one_disk_root_without_cross_retrieval() -> anyhow::Result<()> {
    let a = tempfile::tempdir()?;
    let b = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(a.path().join("same.rs"), "fn collection_a() {}\n")?;
    fs::write(b.path().join("same.rs"), "fn collection_b() {}\n")?;
    let ka = Kernel::open_collection(a.path(), Some(state.path().into()), "project-a")?;
    let kb = Kernel::open_collection(b.path(), Some(state.path().into()), "project-b")?;
    ka.index()?;
    kb.index()?;
    assert_eq!(ka.search(&query("collection_a"))?.collection, "project-a");
    assert!(ka.search(&query("collection_b"))?.hits.is_empty());
    assert!(kb.search(&query("collection_a"))?.hits.is_empty());
    assert!(kb.read_file("../project-a/same.rs", 1, 1, 100).is_err());
    assert!(Kernel::open_collection(a.path(), Some(state.path().into()), "../escape").is_err());
    assert!(Kernel::open_collection(a.path(), Some(state.path().into()), "Project-A").is_err());
    let reopened = Kernel::open_collection(a.path(), Some(state.path().into()), "project-a")?;
    assert_eq!(reopened.status()?.files, 1);
    let usage = ka.collections()?;
    assert_eq!(usage.len(), 2);
    assert!(usage.iter().all(|c| c.disk_bytes > 0));
    let store = FileStore::new(state.path().join("collections/project-b"));
    let lock = store.lock()?;
    assert!(ka.remove_collection("project-b").is_err());
    assert!(kb.status().is_ok());
    drop(lock);
    assert!(
        ka.remove_collection("project-b").is_err(),
        "a live persistent reader must prevent collection removal"
    );
    drop(kb);
    let removed = ka.remove_collection("project-b")?;
    assert_eq!(removed.status, "removed-derived-index");
    assert!(b.path().join("same.rs").exists());
    assert!(
        Kernel::open_collection(b.path(), Some(state.path().into()), "project-b")?
            .status()
            .is_err()
    );
    assert!(ka.status().is_ok());
    assert!(ka.remove_collection("../project-a").is_err());
    Ok(())
}

#[test]
fn facts_are_specialized_and_reference_results_disclose_resolution() -> anyhow::Result<()> {
    use airk_kernel::FactQuery;
    let root = tempfile::tempdir()?;
    fs::write(
        root.path().join("a.py"),
        "import os\n# authenticate is mentioned\ndef authenticate():\n    return True\n",
    )?;
    let kernel = Kernel::open(root.path(), None)?;
    kernel.index()?;
    let facts = kernel.facts(&FactQuery {
        path_prefix: String::new(),
        name: Some("authenticate".into()),
        kind: Some("definition".into()),
        language: Some("python".into()),
        exact_name: true,
        limit: 10,
        max_bytes: 8192,
        path_exact: false,
        document_only: false,
    })?;
    assert_eq!(facts.facts.len(), 1);
    assert_eq!(facts.facts[0].citation.start_line, 3);
    let refs = kernel.references("authenticate", "", 10, 8192)?;
    assert!(refs.resolution.contains("textual"));
    assert_eq!(refs.evidence.hits.len(), 2);
    assert_eq!(kernel.file_info("a.py")?.language, "python");
    assert_eq!(
        kernel.context("a.py", 3, 1, 1, 8192)?.hits[0].text,
        "# authenticate is mentioned\ndef authenticate():\n    return True\n"
    );
    Ok(())
}

#[test]
fn document_categories_roundtrip_with_metadata_budgets_and_extractor_refresh() -> anyhow::Result<()>
{
    use airk_kernel::FactQuery;
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(
        root.path().join("requirements.md"),
        "# Design\n\n## Requirements\n\nREQ-1: The UI must be accessible within 200 ms by 13 October 2026.\n",
    )?;
    fs::write(
        root.path().join("code.py"),
        "# The UI must be accessible by 2026-10-13.\ndef render():\n    pass\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(state.path().to_path_buf()))?;
    let first = kernel.index()?;
    let fact_query = |kind: &str, max_bytes| FactQuery {
        path_prefix: String::new(),
        name: None,
        kind: Some(kind.into()),
        language: None,
        exact_name: false,
        limit: 100,
        max_bytes,
        path_exact: false,
        document_only: true,
    };
    let reopened = Kernel::open(root.path(), Some(state.path().to_path_buf()))?;
    let requirements = reopened.facts(&fact_query("requirement", 8192))?;
    assert_eq!(requirements.facts.len(), 1);
    let fact = &requirements.facts[0];
    assert_eq!(fact.citation.path, "requirements.md");
    assert_eq!(fact.citation.start_line, 5);
    let details = fact.document.as_ref().expect("details");
    assert_eq!(details.requirement_type.as_deref(), Some("mixed"));
    assert_eq!(details.dates[0].normalized.as_deref(), Some("2026-10-13"));
    assert_eq!(details.quantities[0].value, "200");
    let limited = reopened.facts(&fact_query("requirement", 128))?;
    assert!(limited.facts.is_empty());
    assert!(
        limited.truncated,
        "metadata must count against the fact budget"
    );
    let dates = reopened.facts(&fact_query("date", 8192))?;
    assert_eq!(dates.facts.len(), 1);
    assert_eq!(
        dates.facts[0].citation.content_hash,
        fact.citation.content_hash
    );
    assert_eq!(kernel.index()?.reused, 2);
    let store = FileStore::new(state.path().join("collections/default"));
    let mut old = store.load()?.expect("snapshot");
    let catalog = store.catalog()?.expect("catalog");
    for artifact in old.artifacts.values_mut() {
        *artifact = catalog.load_artifact(&artifact.path)?;
        assert!(
            artifact.retained.is_none(),
            "legacy fixture is fully hydrated"
        );
    }
    old.schema_version = airk_core::SCHEMA_VERSION;
    old.storage_format = 0;
    old.persistent_indexes = false;
    old.extractor_version = "native-facts/2".into();
    old.artifacts
        .get_mut("requirements.md")
        .expect("artifact")
        .facts
        .clear();
    let head = store.expected_head()?;
    assert!(
        store.save(&old).is_err(),
        "legacy writes cannot replace v2 evidence"
    );
    assert_eq!(store.expected_head()?, head);
    assert_eq!(kernel.status()?.generation, first.generation);
    let legacy_state = tempfile::tempdir()?;
    let legacy_store = FileStore::new(legacy_state.path().join("collections/default"));
    legacy_store.save(&old)?;
    let legacy_kernel = Kernel::open(root.path(), Some(legacy_state.path().into()))?;
    assert!(
        legacy_kernel
            .facts(&fact_query("requirement", 8192))?
            .facts
            .is_empty()
    );
    let refreshed = legacy_kernel.index()?;
    assert_eq!(refreshed.indexed, 2);
    assert_eq!(refreshed.reused, 0);
    assert_eq!(refreshed.generation, first.generation + 1);
    assert_eq!(
        Kernel::open(root.path(), Some(legacy_state.path().into()))?
            .facts(&fact_query("requirement", 8192))?
            .facts
            .len(),
        1
    );
    assert_eq!(
        legacy_kernel.status()?.extractor_version,
        airk_core::EXTRACTOR_VERSION
    );
    Ok(())
}

#[test]
fn removed_and_rebuilt_collection_cannot_reuse_old_worker_candidates() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    fs::write(root.path().join("old.rs"), "fn old_symbol() {}")?;
    let worker = Kernel::open(root.path(), None)?;
    worker.index()?;
    assert_eq!(worker.search(&query("old_symbol"))?.hits.len(), 1);
    worker.remove_collection("default")?;
    fs::remove_file(root.path().join("old.rs"))?;
    fs::write(root.path().join("new.rs"), "fn new_symbol() {}")?;
    let writer = Kernel::open(root.path(), None)?;
    assert_eq!(writer.index()?.generation, 1);
    assert!(worker.search(&query("old_symbol"))?.hits.is_empty());
    assert_eq!(
        worker.search(&query("new_symbol"))?.hits[0].citation.path,
        "new.rs"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn non_utf8_roots_cannot_share_a_lossy_collection_identity() -> anyhow::Result<()> {
    use std::os::unix::ffi::OsStringExt;
    let parent = tempfile::tempdir()?;
    let data = parent.path().join("state");
    let roots = [0x80, 0x81].map(|suffix| {
        parent
            .path()
            .join(std::ffi::OsString::from_vec(vec![b'w', suffix]))
    });
    assert_ne!(roots[0], roots[1]);
    assert_eq!(roots[0].to_string_lossy(), roots[1].to_string_lossy());
    for root in roots {
        // Some Unix filesystems (including the hosted Mac's volume) reject
        // these names themselves. Exercise AIRK's failure path in either case,
        // rather than requiring the filesystem to accept an invalid name.
        let filesystem_rejected_encoding = match fs::create_dir(&root) {
            Ok(()) => false,
            Err(error) if error.raw_os_error() == Some(libc::EILSEQ) => true,
            Err(error) => return Err(error.into()),
        };
        let error = Kernel::open(&root, Some(data.clone()))
            .err()
            .expect("an invalid root must never acquire a collection identity");
        if filesystem_rejected_encoding {
            assert_eq!(error.to_string(), "resolve workspace root");
            let cause = error
                .downcast_ref::<std::io::Error>()
                .expect("filesystem root resolution error");
            assert!(matches!(
                cause.raw_os_error(),
                Some(libc::EILSEQ | libc::ENOENT)
            ));
        } else {
            assert!(error.to_string().contains("non-UTF-8 workspace root"));
        }
        assert!(
            !data.exists(),
            "a rejected root must not create managed state"
        );
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlinks_do_not_import_external_sources() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let external = tempfile::tempdir()?;
    fs::write(external.path().join("private.txt"), "external_secret")?;
    std::os::unix::fs::symlink(
        external.path().join("private.txt"),
        root.path().join("link.txt"),
    )?;
    std::os::unix::fs::symlink(external.path(), root.path().join("linked-dir"))?;
    let kernel = Kernel::open(root.path(), None)?;
    kernel.index()?;
    assert!(kernel.search(&query("external_secret"))?.hits.is_empty());
    Ok(())
}
