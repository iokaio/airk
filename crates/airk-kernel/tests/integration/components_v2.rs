// SPDX-License-Identifier: Apache-2.0
//! Activate after composite payload retention and mapped citation binding are wired.
use airk_ingest::adapters::profile::{ExtractionPolicy, RuleMatch, SelectionRule};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::{Context, Result};
use std::fs;

fn qualify(path: &str, moved: &str, source: &str, expected_marker: &str) -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let source = source.replace("\r\n", "\n").replace('\n', "\r\n");
    fs::write(root.path().join(path), &source)?;
    fs::write(
        root.path().join("unchanged.rs"),
        "pub fn unchanged() -> u32 { 41 }\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    assert_eq!(kernel.index()?.indexed, 2);
    let store = FileStore::new(data.path().join("collections/default"));
    let before = store.catalog()?.context("catalog")?;
    let artifact = &before.snapshot.artifacts[path];
    assert_eq!(store.read_retained_source(artifact)?, source.as_bytes());
    let payload = store.load_extraction_payload(artifact)?;
    assert_eq!(payload.regions.len(), 2);
    assert!(
        payload
            .regions
            .iter()
            .any(|region| region.text.contains("café"))
    );
    assert!(
        payload
            .facts
            .iter()
            .any(|fact| fact.region.is_some() && fact.name == "save")
    );
    assert!(
        payload
            .facts
            .iter()
            .any(|fact| fact.kind == expected_marker)
    );
    let outline = kernel.outline(path, 100, 65536)?;
    assert!(outline.facts.iter().any(|fact| fact.name == "save"));
    assert!(outline.facts.iter().all(|fact| fact.citation.path == path));
    let warm = kernel.index()?;
    assert_eq!((warm.indexed, warm.reused), (0, 2));
    let changed_source = source.replace("return title;", "return title + ' edited';");
    assert_ne!(source, changed_source);
    fs::write(root.path().join(path), &changed_source)?;
    let edited = kernel.index()?;
    assert_eq!((edited.indexed, edited.reused), (1, 1));
    let changed = store.catalog()?.context("changed catalog")?;
    let after_payload = store.load_extraction_payload(&changed.snapshot.artifacts[path])?;
    assert_eq!(after_payload.regions.len(), payload.regions.len());
    assert_eq!(
        payload
            .regions
            .iter()
            .zip(&after_payload.regions)
            .filter(|(before, after)| before.text != after.text)
            .count(),
        1,
        "only the edited child text changes"
    );
    assert_eq!(
        before.inventory.as_ref().unwrap().files["unchanged.rs"].state,
        changed.inventory.as_ref().unwrap().files["unchanged.rs"].state
    );
    drop(changed);
    fs::rename(root.path().join(path), root.path().join(moved))?;
    let moved_report = kernel.index()?;
    assert_eq!((moved_report.indexed, moved_report.reused), (0, 2));
    let mapped = store.catalog()?.context("moved catalog")?;
    assert_eq!(
        store.read_retained_source(&mapped.snapshot.artifacts[moved])?,
        changed_source.as_bytes()
    );
    assert!(
        kernel
            .outline(moved, 100, 65536)?
            .facts
            .iter()
            .all(|fact| fact.citation.path == moved)
    );
    kernel.configure_extraction_policy(ExtractionPolicy {
        version: 1,
        rules: vec![SelectionRule {
            selector: RuleMatch::Exact { path: moved.into() },
            case_sensitive: true,
            adapter_id: "text".into(),
        }],
    })?;
    let overridden = kernel.index()?;
    assert_eq!((overridden.indexed, overridden.reused), (1, 1));
    Ok(())
}

#[test]
fn vue_children_and_setup_candidates_survive_child_edits_moves_and_profile_changes() -> Result<()> {
    qualify(
        "Original.vue",
        "Moved.vue",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../airk-ingest/tests/fixtures/components/Original.vue"
        )),
        "script_setup",
    )
}

#[test]
fn svelte_module_and_instance_regions_keep_source_identity() -> Result<()> {
    qualify(
        "Original.svelte",
        "Moved.svelte",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../airk-ingest/tests/fixtures/components/Original.svelte"
        )),
        "script_module",
    )
}
