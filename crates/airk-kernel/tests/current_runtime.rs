// SPDX-License-Identifier: Apache-2.0
use airk_core::SearchQuery;
use airk_kernel::Kernel;
use anyhow::Result;

#[test]
#[ignore = "large repository capacity qualification; run explicitly in release mode"]
fn twelve_thousand_files_publish_reopen_update_and_reclaim() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    for file in 0..12_000 {
        std::fs::write(
            root.path().join(format!("fixture_{file:05}.rs")),
            format!("pub fn fixture_{file:05}() -> usize {{ {file} }}\n"),
        )?;
    }
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    let cold = kernel.index()?;
    assert!(cold.maintenance_error.is_none(), "{cold:?}");
    assert_eq!(kernel.status()?.files, 12_000);
    let current = cold.current.unwrap();
    assert_eq!(current.changed_artifacts, 12_000);
    println!("large repository cold: {current:?}");
    drop(kernel);
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    assert_eq!(kernel.status()?.files, 12_000);
    assert_eq!(kernel.index()?.current.unwrap().changed_artifacts, 0);
    std::fs::write(
        root.path().join("fixture_00000.rs"),
        "pub fn changed_fixture() {}\n",
    )?;
    let changed = kernel.index()?;
    assert!(changed.maintenance_error.is_none());
    assert_eq!(changed.current.unwrap().changed_artifacts, 1);
    let result = kernel.search(&SearchQuery {
        text: "changed_fixture".into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 4096,
    })?;
    assert_eq!(result.hits[0].citation.path, "fixture_00000.rs");
    std::fs::remove_file(root.path().join("fixture_11999.rs"))?;
    let removed = kernel.index()?;
    assert!(removed.maintenance_error.is_none());
    assert_eq!(kernel.status()?.files, 11_999);
    Ok(())
}

#[test]
fn shared_embedding_survives_more_than_128_file_occurrences() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    model(&data.path().join("models/local"), 2)?;
    for file in 0..256 {
        std::fs::write(
            root.path().join(format!("alias_{file:03}.txt")),
            "authenticate\n",
        )?;
    }
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    assert_eq!(kernel.status()?.files, 256);
    assert_eq!(kernel.status()?.indexed_vectors, 1);
    let query = SearchQuery {
        text: "login".into(),
        path_prefix: Some("alias_255.txt".into()),
        limit: 1,
        max_bytes: 4096,
    };
    assert_eq!(
        kernel.semantic_search(&query)?.hits[0].citation.path,
        "alias_255.txt"
    );
    std::fs::remove_file(root.path().join("alias_000.txt"))?;
    kernel.index()?;
    assert_eq!(kernel.status()?.indexed_vectors, 1);
    assert_eq!(
        kernel.semantic_search(&query)?.hits[0].citation.path,
        "alias_255.txt"
    );
    Ok(())
}

#[test]
fn public_runtime_indexes_updates_and_reads_current_evidence() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("source");
    std::fs::create_dir(&root)?;
    std::fs::write(root.join("a.txt"), "alpha original\n")?;
    std::fs::write(root.join("b.txt"), "beta retained\n")?;
    let kernel = Kernel::open(&root, Some(directory.path().join("state")))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let before = kernel.status()?;
    assert_eq!(before.files, 2);
    assert!(
        directory
            .path()
            .join("state/collections-1.2/default/current.sqlite3")
            .is_file()
    );
    assert!(
        !directory
            .path()
            .join("state/collections-1.2/default/index.sqlite3")
            .exists()
    );
    let query = SearchQuery {
        text: "alpha".into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 4096,
    };
    assert_eq!(kernel.search(&query)?.hits[0].text, "alpha original\n");
    kernel.index()?;
    assert_eq!(kernel.status()?.publication, before.publication);
    std::fs::write(root.join("a.txt"), "gamma changed\n")?;
    kernel.index()?;
    assert!(kernel.search(&query)?.hits.is_empty());
    assert_eq!(
        kernel.read_file("b.txt", 1, 1, 4096)?.hits[0].text,
        "beta retained\n"
    );
    std::fs::remove_file(root.join("a.txt"))?;
    kernel.index()?;
    assert_eq!(kernel.list_files("", 10, 4096)?.paths, ["b.txt"]);
    assert_eq!(kernel.status()?.files, 1);
    Ok(())
}

fn model(directory: &std::path::Path, dimensions: usize) -> Result<()> {
    use sha2::{Digest, Sha256};
    std::fs::create_dir_all(directory)?;
    let mut vector = vec![0.; dimensions];
    vector[0] = 1.;
    let vocabulary =
        serde_json::to_vec(&serde_json::json!({"authenticate":vector,"login":vector}))?;
    std::fs::write(directory.join("vocabulary.json"), &vocabulary)?;
    std::fs::write(
        directory.join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema":1,"name":"original-current-runtime-fixture","license":"Apache-2.0",
            "dimensions":dimensions,"sha256":format!("{:x}", Sha256::digest(&vocabulary))
        }))?,
    )?;
    Ok(())
}

#[test]
fn model_replacement_full_rebuild_and_empty_policy_commit_are_current() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let initial = kernel.status()?.publication.unwrap().token;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    kernel.index()?;
    assert_ne!(kernel.status()?.publication.unwrap().token, initial);
    assert!(!kernel.extraction_policy()?.pending_reindex);
    std::fs::write(root.path().join("auth.txt"), "authenticate\n")?;
    model(&data.path().join("models/local"), 2)?;
    kernel.configure_vectors(true)?;
    kernel.index()?;
    let query = SearchQuery {
        text: "login".into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 4096,
    };
    assert_eq!(
        kernel.semantic_search(&query)?.hits[0].citation.path,
        "auth.txt"
    );
    let before = kernel.status()?.publication.unwrap().token;
    model(&data.path().join("models/local"), 4)?;
    kernel.index()?;
    assert_ne!(kernel.status()?.publication.unwrap().token, before);
    assert_eq!(kernel.semantic_search(&query)?.hits.len(), 1);
    kernel.reindex()?;
    assert_eq!(kernel.semantic_search(&query)?.hits.len(), 1);
    kernel.configure_vectors(false)?;
    kernel.index()?;
    assert_eq!(kernel.status()?.indexed_vectors, 0);
    assert_eq!(kernel.collections()?[0].indexed_files, Some(1));
    kernel.remove_collection("default")?;
    assert!(root.path().join("auth.txt").is_file());
    Ok(())
}

#[test]
fn old_namespace_is_untouched_and_exact_tokens_and_cursors_expire() -> Result<()> {
    use airk_core::{discovery::DiscoveryOptions, evidence::EvidenceReadOptions};
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let old = data.path().join("collections/default");
    std::fs::create_dir_all(&old)?;
    std::fs::write(old.join("index.sqlite3"), "old index sentinel")?;
    std::fs::write(old.join("vector-policy.json"), "old policy sentinel")?;
    std::fs::write(root.path().join("a.txt"), "alpha\n")?;
    std::fs::write(root.path().join("b.txt"), "beta\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    assert!(kernel.vectors_enabled()?);
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let first = kernel.read_file_conditional("a.txt", 1, 1, 4096, &Default::default())?;
    let revision = first.read.publication.unwrap().token;
    let page = kernel.list_files_page("", 1, 4096, &Default::default())?;
    let cursor = page.page.cursor;
    std::fs::write(root.path().join("b.txt"), "gamma\n")?;
    kernel.index()?;
    assert!(
        kernel
            .read_file_conditional(
                "a.txt",
                1,
                1,
                4096,
                &EvidenceReadOptions {
                    revision: Some(revision),
                    ..Default::default()
                }
            )
            .is_err()
    );
    assert!(
        kernel
            .list_files_page(
                "",
                1,
                4096,
                &DiscoveryOptions {
                    cursor,
                    ..Default::default()
                }
            )
            .is_err()
    );
    let unchanged = kernel.read_file_conditional(
        "a.txt",
        1,
        1,
        4096,
        &EvidenceReadOptions {
            if_evidence: first.read.validator,
            ..Default::default()
        },
    )?;
    assert!(unchanged.content.is_none());
    assert_eq!(
        std::fs::read_to_string(old.join("index.sqlite3"))?,
        "old index sentinel"
    );
    assert_eq!(
        std::fs::read_to_string(old.join("vector-policy.json"))?,
        "old policy sentinel"
    );
    assert!(
        kernel
            .history_availability(&airk_kernel::HistoryAvailabilityRequest {
                revision: None,
                cursor: None,
                limit: 1,
                max_bytes: 4096
            })
            .is_err()
    );
    assert!(kernel.checkpoints().is_err());
    Ok(())
}

#[test]
fn one_file_publication_work_does_not_expand_with_unchanged_corpus() -> Result<()> {
    for count in [8, 64] {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        for file in 0..count {
            std::fs::write(
                root.path().join(format!("original-{file:03}.txt")),
                format!("original stable fixture {file}\n"),
            )?;
        }
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.configure_vectors(false)?;
        let cold = kernel.index()?;
        assert_eq!(cold.current.unwrap().changed_artifacts, count);
        assert!(cold.maintenance_error.is_none());
        assert_eq!(kernel.index()?.current.unwrap().changed_artifacts, 0);
        std::fs::write(
            root.path().join("original-000.txt"),
            "replacement exact fixture\n",
        )?;
        let edit = kernel.index()?;
        assert_eq!(edit.indexed, 1);
        assert_eq!(edit.reused, count - 1);
        let work = edit.current.unwrap();
        assert_eq!(
            (
                work.changed_artifacts,
                work.lexical_documents,
                work.graph_records
            ),
            (1, 1, 0)
        );
        assert!(edit.maintenance_error.is_none());
        let directory = data.path().join("collections-1.2/default");
        assert_eq!(
            std::fs::read_dir(directory.join("objects/sources"))?.count(),
            count
        );
        assert_eq!(
            std::fs::read_dir(directory.join("current-metadata"))?.count(),
            count
        );
        assert_eq!(kernel.maintain_current()?.removed_files, 0);
    }
    Ok(())
}

#[test]
fn interrupted_runtime_bootstrap_completes_before_any_source_scan() -> Result<()> {
    #[derive(serde::Serialize)]
    struct Recipe {
        head: airk_store::current::CurrentHead,
        manifest: airk_retrieval::current_lexical::LexicalManifest,
        configuration: airk_store::current::collection::CollectionConfiguration,
    }
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    std::fs::write(
        root.path().join("unscanned.txt"),
        "source must remain unscanned",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.initialize()?;
    let store = airk_store::FileStore::new(data.path().join("collections-1.2/default"));
    let owner: serde_json::Value = serde_json::from_slice(&std::fs::read(
        store.directory().join("current-owner.json"),
    )?)?;
    let scope = serde_json::from_value(owner["scope"].clone())?;
    let read = store.current_read(&scope)?;
    let mut head = read.head().clone();
    let manifest = airk_retrieval::current_lexical::LexicalManifest::decode(
        &read.lexical_manifest()?,
        head.lexical_commit,
    )?;
    let configuration = read.collection_state()?.unwrap().configuration;
    head.sequence = 0;
    head.artifact_root = airk_store::current::map::empty_root(&scope)?;
    drop(read);
    let writer = store.lock()?;
    store.write_runtime_bootstrap(
        &writer,
        &serde_json::to_vec(&Recipe {
            head,
            manifest,
            configuration,
        })?,
    )?;
    // Recreate the durable boundary after the recipe, before SQLite ownership.
    for name in [
        "current.sqlite3",
        "current-owner.json",
        "current-bootstrap.json",
    ] {
        std::fs::remove_file(store.directory().join(name))?;
    }
    drop(writer);
    drop(kernel);
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    assert_eq!(reopened.status()?.files, 0);
    assert!(store.runtime_bootstrap()?.is_none());
    let token = reopened.status()?.publication.unwrap().token;
    assert_eq!(
        Kernel::open(root.path(), Some(data.path().into()))?
            .status()?
            .publication
            .unwrap()
            .token,
        token
    );
    reopened.index()?;
    assert_eq!(reopened.status()?.files, 1);
    assert_eq!(
        std::fs::read_to_string(root.path().join("unscanned.txt"))?,
        "source must remain unscanned"
    );
    Ok(())
}
