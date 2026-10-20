// SPDX-License-Identifier: Apache-2.0
//! Activate only after the first P10 profile batch is admitted.
use super::current_inspection::InspectCurrent;
use airk_ingest::adapters::profile::{ExtractionPolicy, RuleMatch, SelectionRule};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::{Context, Result};
use std::fs;

fn qualify(path: &str, moved: &str, source: &str, expected: &str) -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let source = source.replace("\r\n", "\n").replace('\n', "\r\n");
    fs::write(root.path().join(path), &source)?;
    fs::write(
        root.path().join("unrelated.rs"),
        "pub fn unaffected() -> u32 {17}\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    assert_eq!(kernel.index()?.indexed, 2);
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let catalog = store.current_test_catalog()?.context("catalog")?;
    let artifact = &catalog.snapshot.artifacts[path];
    assert_eq!(store.read_retained_source(artifact)?, source.as_bytes());
    let before = catalog.inventory.as_ref().unwrap().files[path]
        .state
        .clone();
    let untouched = catalog.inventory.as_ref().unwrap().files["unrelated.rs"]
        .state
        .clone();
    let facts = kernel.outline(path, 100, 65_536)?;
    assert!(facts.facts.iter().any(|f| f.name.contains(expected)));
    if path.ends_with(".Rmd") {
        let payload = store.load_extraction_payload(artifact)?;
        assert_eq!(payload.regions.len(), 2);
        assert!(
            payload
                .facts
                .iter()
                .any(|f| f.region.is_some() && f.name == "render")
        );
        assert!(
            !payload
                .facts
                .iter()
                .any(|f| f.kind == "definition" && f.name == "phantom")
        );
    } else if !path.ends_with(".gradle") {
        let key = facts
            .facts
            .iter()
            .find_map(|f| f.continuity_key)
            .context("qualified native declaration key")?;
        assert_eq!(kernel.continuity_facts(key, 1, 4096)?.facts.len(), 1);
    }
    let warm = kernel.index()?;
    assert_eq!((warm.indexed, warm.reused), (0, 2));
    fs::rename(root.path().join(path), root.path().join(moved))?;
    let moved_report = kernel.index()?;
    assert_eq!((moved_report.indexed, moved_report.reused), (0, 2));
    let after = store.current_test_catalog()?.context("moved catalog")?;
    assert_eq!(
        after.inventory.as_ref().unwrap().files[moved]
            .state
            .payload_digest,
        before.payload_digest
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
    let after = store.current_test_catalog()?.context("changed catalog")?;
    assert_ne!(
        after.inventory.as_ref().unwrap().files[moved]
            .state
            .profile_id,
        before.profile_id
    );
    assert_eq!(
        after.inventory.as_ref().unwrap().files["unrelated.rs"].state,
        untouched
    );
    Ok(())
}
macro_rules! check {
    ($name:ident,$path:literal,$moved:literal,$expected:literal) => {
        #[test]
        fn $name() -> Result<()> {
            qualify(
                $path,
                $moved,
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../airk-ingest/tests/fixtures/languages-b-first/",
                    $path
                )),
                $expected,
            )
        }
    };
}
check!(r_pipeline, "original.R", "moved.R", "render");
check!(perl_pipeline, "original.pl", "moved.pl", "render");
check!(
    groovy_pipeline,
    "original.groovy",
    "moved.groovy",
    "Archive"
);
check!(
    gradle_pipeline,
    "original.gradle",
    "moved.gradle",
    "example:archive"
);
check!(rmd_mapped_pipeline, "original.Rmd", "moved.Rmd", "REQ-920");
