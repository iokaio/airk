// SPDX-License-Identifier: Apache-2.0
//! Activate only after SCSS grammar admission and profile wiring.
use super::current_inspection::InspectCurrent;
use airk_core::{EvidenceBasis, FactQuery};
use airk_ingest::adapters::profile::{ExtractionPolicy, RuleMatch, SelectionRule};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::{Context, Result};
use std::fs;

#[test]
fn scss_original_locations_profile_invalidation_and_rename_reuse() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../airk-ingest/tests/fixtures/scss/original.scss"
    ))
    .replace("\r\n", "\n")
    .replace('\n', "\r\n");
    fs::write(root.path().join("original.scss"), &source)?;
    fs::write(
        root.path().join("unchanged.rs"),
        "pub fn unchanged() -> u32 { 19 }\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    assert_eq!(kernel.index()?.indexed, 2);
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let before = store.current_test_catalog()?.context("catalog")?;
    assert_eq!(
        store.read_retained_source(&before.snapshot.artifacts["original.scss"])?,
        source.as_bytes()
    );
    let facts = kernel.outline("original.scss", 100, 65536)?;
    for kind in [
        "selector", "variable", "mixin", "include", "function", "use", "forward", "import",
    ] {
        assert!(
            facts.facts.iter().any(|fact| fact.kind == kind),
            "missing {kind}"
        );
    }
    for fact in &facts.facts {
        let code = fact.code.as_ref().context("SCSS syntax evidence")?;
        assert_eq!(code.basis, EvidenceBasis::Syntax);
        assert_eq!(code.producer_version, "composite-facts/1");
        assert!(code.fingerprints.is_none());
        assert!(fact.continuity_key.is_none());
        let span = code.span.as_ref().context("SCSS byte span")?;
        assert!(source[span.start_byte..span.end_byte].contains(&fact.name));
        assert_eq!(
            fact.citation.start_line,
            source[..span.start_byte]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1
        );
    }
    let bounded = kernel.facts(&FactQuery {
        path_prefix: "original.scss".into(),
        name: None,
        kind: None,
        language: None,
        exact_name: false,
        limit: 1,
        max_bytes: 4096,
        path_exact: true,
        document_only: false,
    })?;
    assert!(bounded.facts.len() <= 1 && bounded.truncated);
    let warm = kernel.index()?;
    assert_eq!((warm.indexed, warm.reused), (0, 2));
    kernel.configure_extraction_policy(ExtractionPolicy {
        detail: None,
        version: 1,
        rules: vec![SelectionRule {
            selector: RuleMatch::Exact {
                path: "original.scss".into(),
            },
            case_sensitive: true,
            adapter_id: "css".into(),
        }],
    })?;
    let changed = kernel.index()?;
    assert_eq!((changed.indexed, changed.reused), (1, 1));
    let changed_catalog = store.current_test_catalog()?.context("changed catalog")?;
    assert_ne!(
        before.inventory.as_ref().unwrap().files["original.scss"]
            .state
            .profile_id,
        changed_catalog.inventory.as_ref().unwrap().files["original.scss"]
            .state
            .profile_id
    );
    assert_eq!(
        before.inventory.as_ref().unwrap().files["unchanged.rs"].state,
        changed_catalog.inventory.as_ref().unwrap().files["unchanged.rs"].state
    );
    drop(changed_catalog);
    kernel.configure_extraction_policy(ExtractionPolicy {
        detail: None,
        version: 1,
        rules: vec![],
    })?;
    kernel.index()?;
    fs::rename(
        root.path().join("original.scss"),
        root.path().join("moved.scss"),
    )?;
    let moved = kernel.index()?;
    assert_eq!((moved.indexed, moved.reused), (0, 2));
    let after = store.current_test_catalog()?.context("moved catalog")?;
    assert_eq!(
        before.inventory.as_ref().unwrap().files["original.scss"]
            .state
            .payload_digest,
        after.inventory.as_ref().unwrap().files["moved.scss"]
            .state
            .payload_digest
    );
    Ok(())
}
