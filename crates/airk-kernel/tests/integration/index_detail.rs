// SPDX-License-Identifier: Apache-2.0
use airk_core::{FactQuery, extraction::IndexDetail};
use airk_kernel::{Kernel, extraction::ExtractionPolicy, limits::ScanLimits};
use airk_store::FileStore;
use anyhow::Result;
use std::fs;

fn facts() -> FactQuery {
    FactQuery {
        path_prefix: String::new(),
        name: None,
        kind: None,
        language: None,
        exact_name: false,
        path_exact: false,
        document_only: false,
        limit: 100,
        max_bytes: 65536,
    }
}

#[test]
fn levels_rebuild_cache_preserve_citations_and_report_published_coverage() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let source = "pub fn detail_fixture() { println!(\"exact original text\"); other(); }\n";
    fs::write(root.path().join("sample.rs"), source)?;
    fs::write(
        root.path().join("notes.md"),
        "# Original heading\n\nThe client must keep exact citations.\n",
    )?;
    fs::write(
        root.path().join("host.md"),
        "# Embedded\n\n```javascript\nfunction embedded() { console.log('original'); }\n```\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    assert_eq!(
        kernel.extraction_policy()?.configured_detail,
        IndexDetail::Balanced
    );
    kernel.index()?;
    let before = kernel.facts(&facts())?;
    assert_eq!(before.indexing.level, IndexDetail::Balanced);
    assert!(before.facts.iter().any(|f| f.kind == "definition"));
    assert!(!before.facts.iter().any(|f| f.kind == "call"));
    assert_eq!(kernel.index()?.reused, 3);
    let store = FileStore::new(data.path().join("collections/default"));
    let balanced = store.catalog()?.unwrap();
    let first_profile = balanced.inventory.as_ref().unwrap().files["sample.rs"]
        .state
        .profile_id;
    drop(balanced);
    kernel.configure_index_detail(IndexDetail::Detailed)?;
    assert!(kernel.extraction_policy()?.pending_reindex);
    assert_eq!(
        kernel.facts(&facts())?.indexing.level,
        IndexDetail::Balanced
    );
    let report = kernel.index()?;
    assert_eq!(report.indexed, 3);
    assert_eq!(report.reused, 0);
    let detailed = kernel.facts(&facts())?;
    assert_eq!(detailed.indexing.level, IndexDetail::Detailed);
    assert!(detailed.facts.iter().any(|f| f.kind == "call"));
    let catalog = store.catalog()?.unwrap();
    assert_ne!(
        catalog.inventory.as_ref().unwrap().files["sample.rs"]
            .state
            .profile_id,
        first_profile
    );
    drop(catalog);
    kernel.configure_index_detail(IndexDetail::Essential)?;
    // A rejected rebuild must retain the previously published level and facts.
    let limits = kernel.scan_limits()?;
    kernel.configure_scan_limits(ScanLimits { files: 1, ..limits })?;
    assert!(kernel.index().is_err());
    assert_eq!(
        kernel.facts(&facts())?.indexing.level,
        IndexDetail::Detailed
    );
    assert!(kernel.extraction_policy()?.pending_reindex);
    kernel.configure_scan_limits(limits)?;
    assert_eq!(kernel.index()?.indexed, 3);
    let essential = kernel.facts(&facts())?;
    assert!(essential.facts.is_empty());
    assert_eq!(essential.indexing.level, IndexDetail::Essential);
    assert!(essential.indexing.note.contains("not indexed"));
    let catalog = store.catalog()?.unwrap();
    use airk_core::ArtifactSource;
    let artifact = catalog.load_artifact("sample.rs")?;
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|c| c.text.as_str())
            .collect::<String>(),
        source
    );
    assert!(
        catalog
            .inventory
            .as_ref()
            .unwrap()
            .files
            .values()
            .all(|f| f.continuity.is_empty())
    );
    assert_eq!(fs::read_to_string(root.path().join("sample.rs"))?, source);
    assert_eq!(
        Kernel::open(root.path(), Some(data.path().into()))?
            .extraction_policy()?
            .configured_detail,
        IndexDetail::Essential
    );
    let other = Kernel::open_collection(root.path(), Some(data.path().into()), "other")?;
    assert_eq!(
        other.extraction_policy()?.configured_detail,
        IndexDetail::Balanced
    );
    assert_eq!(other.extraction_policy()?.published_detail, None);
    Ok(())
}

#[test]
fn legacy_policy_stays_detailed_and_detail_selection_keeps_adapter_rules() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("sample.rs"),
        "pub fn original() { other(); }\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    let legacy: ExtractionPolicy = serde_json::from_str(
        r#"{"version":1,"rules":[{"selector":{"kind":"exact","path":"sample.rs"},"case_sensitive":true,"adapter_id":"rust"}]}"#,
    )?;
    kernel.configure_extraction_policy(legacy.clone())?;
    kernel.index()?;
    assert_eq!(
        kernel.extraction_policy()?.published_detail,
        Some(IndexDetail::Detailed)
    );
    kernel.configure_index_detail(IndexDetail::Balanced)?;
    assert_eq!(kernel.extraction_policy()?.policy.rules, legacy.rules);
    kernel.index()?;
    assert!(!kernel.extraction_policy()?.pending_reindex);
    Ok(())
}

#[test]
fn warmed_exact_reads_remain_coherent_during_publication_and_cleanup() -> Result<()> {
    use sha2::{Digest, Sha256};
    use std::sync::{Arc, Barrier};
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    let texts: Vec<_> = (0..4)
        .map(|n| format!("pub fn publication_{n}() {{}}\n"))
        .collect();
    fs::write(root.path().join("sample.rs"), &texts[0])?;
    kernel.index()?;
    kernel.configure_retention(airk_store::retention::RetentionPolicy {
        mode: airk_store::retention::RetentionMode::Automatic,
        searchable_predecessors: 0,
        ..Default::default()
    })?;
    let store = FileStore::new(data.path().join("collections/default"));
    let identity = store.catalog()?.unwrap().snapshot.root;
    // Populate semantic-validation cache before the publication/maintenance race.
    assert_eq!(
        store
            .facts(&identity, "default", &facts(), false)?
            .unwrap()
            .facts
            .len(),
        1
    );
    let barrier = Arc::new(Barrier::new(2));
    let reader_barrier = barrier.clone();
    let reader_store = store.clone();
    let reader_texts = texts.clone();
    let reader = std::thread::spawn(move || -> Result<()> {
        reader_barrier.wait();
        for _ in 0..80 {
            let response = reader_store
                .facts(&identity, "default", &facts(), false)?
                .unwrap();
            assert_eq!(
                response.publication.as_ref().unwrap().token.generation,
                response.generation
            );
            assert_eq!(response.facts.len(), 1);
            let fact = &response.facts[0];
            let index: usize = fact.name.strip_prefix("publication_").unwrap().parse()?;
            assert_eq!(fact.citation.path, "sample.rs");
            assert_eq!(
                fact.citation.content_hash,
                format!("{:x}", Sha256::digest(reader_texts[index].as_bytes()))
            );
        }
        Ok(())
    });
    barrier.wait();
    for text in texts.iter().skip(1) {
        fs::write(root.path().join("sample.rs"), text)?;
        kernel.index()?;
        kernel.maintain_retention()?;
    }
    reader.join().expect("reader panicked")?;
    kernel.maintain_retention()?;
    let retained = fs::read_dir(store.directory().join("generations"))?.count();
    assert_eq!(
        retained, 1,
        "verification cache must not pin obsolete generations"
    );
    Ok(())
}
