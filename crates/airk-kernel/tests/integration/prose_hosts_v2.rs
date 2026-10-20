// SPDX-License-Identifier: Apache-2.0
//! Activate after prose-host exclusions and composite payload publication are wired.
use super::current_inspection::InspectCurrent;
use airk_ingest::adapters::profile::{ExtractionPolicy, RuleMatch, SelectionRule};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::{Context, Result};
use std::fs;

fn qualify(
    path: &str,
    moved: &str,
    source: &str,
    expected_regions: usize,
    markdown: bool,
) -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let source = source.replace("\r\n", "\n").replace('\n', "\r\n");
    fs::write(root.path().join(path), &source)?;
    fs::write(
        root.path().join("unchanged.rs"),
        "pub fn unchanged() -> u32 { 23 }\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    assert_eq!(kernel.index()?.indexed, 2);
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let before = store.current_test_catalog()?.context("catalog")?;
    let compact = &before.snapshot.artifacts[path];
    assert_eq!(store.read_retained_source(compact)?, source.as_bytes());
    let payload = store.load_extraction_payload(compact)?;
    assert_eq!(payload.regions.len(), expected_regions);
    assert!(payload.facts.iter().any(|fact| fact.region.is_some()
        && fact.kind == "import"
        && fact.name.contains("ArchiveCard")));
    assert!(!payload.facts.iter().any(|fact| fact.region.is_some()
        && fact.kind == "import"
        && fact.name.contains("not-a-real-import")));
    if markdown {
        assert_eq!(
            payload
                .facts
                .iter()
                .filter(|fact| fact.kind == "requirement" && fact.name.contains("REQ-884"))
                .count(),
            1
        );
        assert!(
            payload
                .facts
                .iter()
                .filter(|fact| fact.document.is_some())
                .all(|fact| !fact.name.starts_with("export const title"))
        );
    }
    let warm = kernel.index()?;
    assert_eq!((warm.indexed, warm.reused), (0, 2));
    let edited_source = source.replace("\"café\"", "\"café revised\"");
    fs::write(root.path().join(path), &edited_source)?;
    let edited = kernel.index()?;
    assert_eq!((edited.indexed, edited.reused), (1, 1));
    let edited_catalog = store.current_test_catalog()?.context("edited catalog")?;
    let edited_payload = store.load_extraction_payload(&edited_catalog.snapshot.artifacts[path])?;
    assert_eq!(
        payload
            .regions
            .iter()
            .zip(&edited_payload.regions)
            .filter(|(old, new)| old.text != new.text)
            .count(),
        1
    );
    assert_eq!(
        before.inventory.as_ref().unwrap().files["unchanged.rs"].state,
        edited_catalog.inventory.as_ref().unwrap().files["unchanged.rs"].state
    );
    drop(edited_catalog);
    fs::rename(root.path().join(path), root.path().join(moved))?;
    let renamed = kernel.index()?;
    assert_eq!((renamed.indexed, renamed.reused), (0, 2));
    assert!(
        kernel
            .outline(moved, 100, 65536)?
            .facts
            .iter()
            .all(|fact| fact.citation.path == moved)
    );
    kernel.configure_extraction_policy(ExtractionPolicy {
        detail: None,
        version: 1,
        rules: vec![SelectionRule {
            selector: RuleMatch::Exact { path: moved.into() },
            case_sensitive: true,
            adapter_id: "text".into(),
        }],
    })?;
    let changed = kernel.index()?;
    assert_eq!((changed.indexed, changed.reused), (1, 1));
    Ok(())
}

#[test]
fn astro_frontmatter_and_explicit_children_keep_mapped_source_through_moves() -> Result<()> {
    qualify(
        "Original.astro",
        "Moved.astro",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../airk-ingest/tests/fixtures/prose-hosts/Original.astro"
        )),
        3,
        false,
    )
}

#[test]
fn mdx_native_islands_preserve_prose_and_do_not_parse_fenced_examples() -> Result<()> {
    qualify(
        "Original.mdx",
        "Moved.mdx",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../airk-ingest/tests/fixtures/prose-hosts/Original.mdx"
        )),
        4,
        true,
    )
}
