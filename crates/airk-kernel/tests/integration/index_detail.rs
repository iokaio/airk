// SPDX-License-Identifier: Apache-2.0
use super::current_inspection::InspectCurrent;
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
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let balanced = store.current_test_catalog()?.unwrap();
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
    let catalog = store.current_test_catalog()?.unwrap();
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
    let catalog = store.current_test_catalog()?.unwrap();
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
fn current_fact_reads_hold_publication_gate_until_request_release() -> Result<()> {
    use super::current_inspection::read_current;
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("sample.rs"),
        "pub fn before_publication() {}\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let request = read_current(&store)?;
    let response = request.native_facts_page(&store, &facts(), false, &Default::default())?;
    assert_eq!(response.facts.len(), 1);
    fs::write(
        root.path().join("sample.rs"),
        "pub fn after_publication() {}\n",
    )?;
    assert!(kernel.index().is_err());
    assert_eq!(
        request
            .native_facts_page(&store, &facts(), false, &Default::default())?
            .facts[0]
            .name,
        "before_publication"
    );
    drop(request);
    kernel.index()?;
    assert_eq!(kernel.facts(&facts())?.facts[0].name, "after_publication");
    assert_eq!(kernel.maintain_current()?.removed_files, 0);
    Ok(())
}
